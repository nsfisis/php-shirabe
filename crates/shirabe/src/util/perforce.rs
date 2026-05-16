//! ref: composer/src/Composer/Util/Perforce.php

use anyhow::Result;
use indexmap::IndexMap;
use shirabe_external_packages::composer::pcre::preg::Preg;
use shirabe_external_packages::symfony::component::process::executable_finder::ExecutableFinder;
use shirabe_external_packages::symfony::component::process::process::Process;
use shirabe_php_shim::{
    chdir, count, date, explode, fclose, feof, fgets, file_get_contents, fopen, fwrite,
    gethostname, json_decode, str_replace_array, strcmp, strlen, strpos, strrpos, substr, time,
    trim, Exception, PhpMixed, PHP_EOL,
};

use crate::io::io_interface::IOInterface;
use crate::util::filesystem::Filesystem;
use crate::util::platform::Platform;
use crate::util::process_executor::ProcessExecutor;

/// @phpstan-type RepoConfig array{unique_perforce_client_name?: string, depot?: string, branch?: string, p4user?: string, p4password?: string}
#[derive(Debug)]
pub struct Perforce {
    pub(crate) path: String,
    pub(crate) p4_depot: Option<String>,
    pub(crate) p4_client: Option<String>,
    pub(crate) p4_user: Option<String>,
    pub(crate) p4_password: Option<String>,
    pub(crate) p4_port: String,
    pub(crate) p4_stream: Option<String>,
    pub(crate) p4_client_spec: String,
    pub(crate) p4_depot_type: Option<String>,
    pub(crate) p4_branch: Option<String>,
    pub(crate) process: ProcessExecutor,
    pub(crate) unique_perforce_client_name: String,
    pub(crate) windows_flag: bool,
    pub(crate) command_result: String,
    pub(crate) io: Box<dyn IOInterface>,
    pub(crate) filesystem: Option<Filesystem>,
}

impl Perforce {
    /// @phpstan-param RepoConfig $repoConfig
    pub fn new(
        repo_config: IndexMap<String, PhpMixed>,
        port: String,
        path: String,
        process: ProcessExecutor,
        is_windows: bool,
        io: Box<dyn IOInterface>,
    ) -> Self {
        let mut this = Self {
            path: String::new(),
            p4_depot: None,
            p4_client: None,
            p4_user: None,
            p4_password: None,
            p4_port: port,
            p4_stream: None,
            p4_client_spec: String::new(),
            p4_depot_type: None,
            p4_branch: None,
            process,
            unique_perforce_client_name: String::new(),
            windows_flag: is_windows,
            command_result: String::new(),
            io,
            filesystem: None,
        };
        this.initialize_path(&path);
        this.initialize(&repo_config);
        this
    }

    /// @phpstan-param RepoConfig $repoConfig
    pub fn create(
        repo_config: IndexMap<String, PhpMixed>,
        port: String,
        path: String,
        process: ProcessExecutor,
        io: Box<dyn IOInterface>,
    ) -> Self {
        Self::new(repo_config, port, path, process, Platform::is_windows(), io)
    }

    pub fn check_server_exists(url: &str, process_executor: &mut ProcessExecutor) -> bool {
        let mut ignored_output = String::new();
        process_executor.execute(
            &vec![
                "p4".to_string(),
                "-p".to_string(),
                url.to_string(),
                "info".to_string(),
                "-s".to_string(),
            ],
            &mut ignored_output,
            None,
        ) == 0
    }

    /// @phpstan-param RepoConfig $repoConfig
    pub fn initialize(&mut self, repo_config: &IndexMap<String, PhpMixed>) {
        self.unique_perforce_client_name = self.generate_unique_perforce_client_name();
        if repo_config.is_empty() {
            return;
        }
        if let Some(value) = repo_config
            .get("unique_perforce_client_name")
            .and_then(|v| v.as_string())
        {
            self.unique_perforce_client_name = value.to_string();
        }

        if let Some(value) = repo_config.get("depot").and_then(|v| v.as_string()) {
            self.p4_depot = Some(value.to_string());
        }
        if let Some(value) = repo_config.get("branch").and_then(|v| v.as_string()) {
            self.p4_branch = Some(value.to_string());
        }
        if let Some(value) = repo_config.get("p4user").and_then(|v| v.as_string()) {
            self.p4_user = Some(value.to_string());
        } else {
            self.p4_user = self.get_p4_variable("P4USER");
        }
        if let Some(value) = repo_config.get("p4password").and_then(|v| v.as_string()) {
            self.p4_password = Some(value.to_string());
        }
    }

    pub fn initialize_depot_and_branch(&mut self, depot: Option<&str>, branch: Option<&str>) {
        if let Some(depot) = depot {
            self.p4_depot = Some(depot.to_string());
        }
        if let Some(branch) = branch {
            self.p4_branch = Some(branch.to_string());
        }
    }

    /// @return non-empty-string
    pub fn generate_unique_perforce_client_name(&self) -> String {
        format!("{}_{}", gethostname(), time())
    }

    pub fn cleanup_client_spec(&mut self) {
        let client = self.get_client();
        let task = vec!["client".to_string(), "-d".to_string(), client];
        let use_p4_client = false;
        let command = self.generate_p4_command(task, use_p4_client);
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let client_spec = self.get_p4_client_spec();
        let file_system = self.get_filesystem();
        file_system.remove(&client_spec);
    }

    /// @param non-empty-string|non-empty-list<string> $command
    pub(crate) fn execute_command(&mut self, command: PhpMixed) -> i64 {
        self.command_result = String::new();

        let cmd_vec: Vec<String> = match &command {
            PhpMixed::List(l) => l
                .iter()
                .filter_map(|v| v.as_string().map(|s| s.to_string()))
                .collect(),
            PhpMixed::String(s) => vec![s.clone()],
            _ => vec![],
        };
        self.process
            .execute(&cmd_vec, &mut self.command_result, None)
    }

    pub fn get_client(&mut self) -> String {
        if self.p4_client.is_none() {
            let stream = self.get_stream();
            let clean_stream_name = str_replace_array(
                &["//".to_string(), "/".to_string(), "@".to_string()],
                &["".to_string(), "_".to_string(), "".to_string()],
                &stream,
            );
            self.p4_client = Some(format!(
                "composer_perforce_{}_{}",
                self.unique_perforce_client_name, clean_stream_name
            ));
        }

        self.p4_client.clone().unwrap_or_default()
    }

    pub(crate) fn get_path(&self) -> &str {
        &self.path
    }

    pub fn initialize_path(&mut self, path: &str) {
        self.path = path.to_string();
        let fs = self.get_filesystem();
        fs.ensure_directory_exists(path);
    }

    pub(crate) fn get_port(&self) -> &str {
        &self.p4_port
    }

    pub fn set_stream(&mut self, stream: &str) {
        self.p4_stream = Some(stream.to_string());
        let index = strrpos(stream, "/");
        // Stream format is //depot/stream, while non-streaming depot is //depot
        if let Some(i) = index {
            if (i as i64) > 2 {
                self.p4_depot_type = Some("stream".to_string());
            }
        }
    }

    pub fn is_stream(&self) -> bool {
        self.p4_depot_type.is_some()
            && strcmp(self.p4_depot_type.as_deref().unwrap_or(""), "stream") == 0
    }

    pub fn get_stream(&mut self) -> String {
        if self.p4_stream.is_none() {
            if self.is_stream() {
                self.p4_stream = Some(format!(
                    "//{}/{}",
                    self.p4_depot.as_deref().unwrap_or(""),
                    self.p4_branch.as_deref().unwrap_or("")
                ));
            } else {
                self.p4_stream = Some(format!("//{}", self.p4_depot.as_deref().unwrap_or("")));
            }
        }

        self.p4_stream.clone().unwrap_or_default()
    }

    pub fn get_stream_without_label(&self, stream: &str) -> String {
        let index = strpos(stream, "@");
        match index {
            None => stream.to_string(),
            Some(idx) => substr(stream, 0, Some(idx as i64)),
        }
    }

    /// @return non-empty-string
    pub fn get_p4_client_spec(&mut self) -> String {
        format!("{}/{}.p4.spec", self.path, self.get_client())
    }

    pub fn get_user(&self) -> Option<String> {
        self.p4_user.clone()
    }

    pub fn set_user(&mut self, user: Option<String>) {
        self.p4_user = user;
    }

    pub fn query_p4_user(&mut self) {
        let _ = self.get_user();
        if strlen(&self.p4_user.clone().unwrap_or_default()) > 0 {
            return;
        }
        self.p4_user = self.get_p4_variable("P4USER");
        if strlen(&self.p4_user.clone().unwrap_or_default()) > 0 {
            return;
        }
        self.p4_user = self
            .io
            .ask("Enter P4 User:".to_string(), PhpMixed::Null)
            .as_string()
            .map(|s| s.to_string());
        let command = if self.windows_flag {
            format!(
                "{} set P4USER={}",
                Self::get_p4_executable(),
                ProcessExecutor::escape(self.p4_user.as_deref().unwrap_or(""))
            )
        } else {
            format!(
                "export P4USER={}",
                ProcessExecutor::escape(self.p4_user.as_deref().unwrap_or(""))
            )
        };
        self.execute_command(PhpMixed::String(command));
    }

    pub(crate) fn get_p4_variable(&mut self, name: &str) -> Option<String> {
        if self.windows_flag {
            let command = format!("{} set", Self::get_p4_executable());
            self.execute_command(PhpMixed::String(command));
            let result = trim(&self.command_result, None);
            let res_array = explode(PHP_EOL, &result);
            for line in &res_array {
                let fields = explode("=", line);
                if strcmp(name, fields.get(0).map(|s| s.as_str()).unwrap_or("")) == 0 {
                    let field1 = fields.get(1).cloned().unwrap_or_default();
                    let index = strpos(&field1, " ");
                    let value = match index {
                        None => field1.clone(),
                        Some(idx) => substr(&field1, 0, Some(idx as i64)),
                    };
                    let value = trim(&value, None);

                    return Some(value);
                }
            }

            return None;
        }

        let command = format!("echo ${}", name);
        self.execute_command(PhpMixed::String(command));
        let result = trim(&self.command_result, None);

        Some(result)
    }

    pub fn query_p4_password(&mut self) -> Option<String> {
        if let Some(ref p) = self.p4_password {
            return Some(p.clone());
        }
        let mut password = self.get_p4_variable("P4PASSWD");
        if strlen(&password.clone().unwrap_or_default()) <= 0 {
            password = self.io.ask_and_hide_answer(format!(
                "Enter password for Perforce user {}: ",
                self.get_user().unwrap_or_default()
            ));
        }
        self.p4_password = password.clone();

        password
    }

    /// @internal
    /// @param non-empty-list<string> $arguments Additional arguments for git rev-list
    /// @return non-empty-list<string>
    pub fn generate_p4_command(
        &mut self,
        arguments: Vec<String>,
        use_client: bool,
    ) -> Vec<String> {
        let mut p4_command: Vec<String> = vec![Self::get_p4_executable()];
        if self.get_user().is_some() {
            p4_command.push("-u".to_string());
            p4_command.push(self.get_user().unwrap_or_default());
        }
        if use_client {
            p4_command.push("-c".to_string());
            p4_command.push(self.get_client());
        }
        p4_command.push("-p".to_string());
        p4_command.push(self.get_port().to_string());

        let mut result = p4_command;
        result.extend(arguments);
        result
    }

    pub fn is_logged_in(&mut self) -> Result<bool> {
        let command =
            self.generate_p4_command(vec!["login".to_string(), "-s".to_string()], false);
        let exit_code = self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        if exit_code != 0 {
            let error_output = self.process.get_error_output().to_string();
            let user = self.get_user().unwrap_or_default();
            let index = strpos(&error_output, &user);
            if index.is_none() {
                let index = strpos(&error_output, "p4");
                if index.is_none() {
                    return Ok(false);
                }
                return Err(Exception {
                    message: format!("p4 command not found in path: {}", error_output),
                    code: 0,
                }
                .into());
            }
            return Err(Exception {
                message: format!("Invalid user name: {}", user),
                code: 0,
            }
            .into());
        }

        Ok(true)
    }

    pub fn connect_client(&mut self) {
        let p4_create_client_command =
            self.generate_p4_command(vec!["client".to_string(), "-i".to_string()], true);

        let mut process = Process::new(
            PhpMixed::List(
                p4_create_client_command
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ),
            None,
            None,
            file_get_contents(&self.get_p4_client_spec()),
            None,
        );
        process.run(None, IndexMap::new());
    }

    pub fn sync_code_base(&mut self, source_reference: Option<&str>) -> Result<()> {
        let prev_dir = Platform::get_cwd(false)?;
        chdir(&self.path);
        let mut p4_sync_command =
            self.generate_p4_command(vec!["sync".to_string(), "-f".to_string()], true);
        if let Some(source_reference) = source_reference {
            p4_sync_command.push(format!("@{}", source_reference));
        }
        self.execute_command(PhpMixed::List(
            p4_sync_command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        chdir(&prev_dir);

        Ok(())
    }

    /// @param resource|false $spec
    pub fn write_client_spec_to_file(&mut self, spec: PhpMixed) {
        fwrite(
            spec.clone(),
            &format!("Client: {}{}{}", self.get_client(), PHP_EOL, PHP_EOL),
            0,
        );
        fwrite(
            spec.clone(),
            &format!(
                "Update: {}{}{}",
                date("Y/m/d H:i:s", None),
                PHP_EOL,
                PHP_EOL
            ),
            0,
        );
        fwrite(
            spec.clone(),
            &format!("Access: {}{}", date("Y/m/d H:i:s", None), PHP_EOL),
            0,
        );
        fwrite(
            spec.clone(),
            &format!(
                "Owner:  {}{}{}",
                self.get_user().unwrap_or_default(),
                PHP_EOL,
                PHP_EOL
            ),
            0,
        );
        fwrite(spec.clone(), &format!("Description:{}", PHP_EOL), 0);
        fwrite(
            spec.clone(),
            &format!(
                "  Created by {} from composer.{}{}",
                self.get_user().unwrap_or_default(),
                PHP_EOL,
                PHP_EOL
            ),
            0,
        );
        fwrite(
            spec.clone(),
            &format!("Root: {}{}{}", self.get_path(), PHP_EOL, PHP_EOL),
            0,
        );
        fwrite(
            spec.clone(),
            &format!(
                "Options:  noallwrite noclobber nocompress unlocked modtime rmdir{}{}",
                PHP_EOL, PHP_EOL
            ),
            0,
        );
        fwrite(
            spec.clone(),
            &format!("SubmitOptions:  revertunchanged{}{}", PHP_EOL, PHP_EOL),
            0,
        );
        fwrite(
            spec.clone(),
            &format!("LineEnd:  local{}{}", PHP_EOL, PHP_EOL),
            0,
        );
        if self.is_stream() {
            fwrite(spec.clone(), &format!("Stream:{}", PHP_EOL), 0);
            let stream_clone = self.p4_stream.clone().unwrap_or_default();
            fwrite(
                spec,
                &format!(
                    "  {}{}",
                    self.get_stream_without_label(&stream_clone),
                    PHP_EOL
                ),
                0,
            );
        } else {
            let stream = self.get_stream();
            let client = self.get_client();
            fwrite(
                spec,
                &format!("View:  {}/...  //{}/... {}", stream, client, PHP_EOL),
                0,
            );
        }
    }

    pub fn write_p4_client_spec(&mut self) -> Result<()> {
        let client_spec = self.get_p4_client_spec();
        let spec = fopen(&client_spec, "w");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.write_client_spec_to_file(spec.clone());
        }));
        if let Err(e) = result {
            fclose(spec);
            return Err(Exception {
                message: format!("{:?}", e),
                code: 0,
            }
            .into());
        }
        fclose(spec);
        Ok(())
    }

    /// @param resource $pipe
    /// @param mixed    $name
    pub(crate) fn read(&self, pipe: PhpMixed, _name: PhpMixed) {
        if feof(pipe.clone()) {
            return;
        }
        let mut line = fgets(pipe.clone());
        while line.is_some() {
            line = fgets(pipe.clone());
        }
    }

    pub fn windows_login(&mut self, password: Option<&str>) -> i64 {
        let command = self.generate_p4_command(vec!["login".to_string(), "-a".to_string()], true);

        let mut process = Process::new(
            PhpMixed::List(
                command
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ),
            None,
            None,
            password.map(|s| s.to_string()),
            None,
        );

        process.run(None, IndexMap::new())
    }

    pub fn p4_login(&mut self) -> Result<()> {
        self.query_p4_user();
        if !self.is_logged_in()? {
            let password = self.query_p4_password();
            if self.windows_flag {
                self.windows_login(password.as_deref());
            } else {
                let command =
                    self.generate_p4_command(vec!["login".to_string(), "-a".to_string()], false);

                let mut process = Process::new(
                    PhpMixed::List(
                        command
                            .into_iter()
                            .map(|s| Box::new(PhpMixed::String(s)))
                            .collect(),
                    ),
                    None,
                    None,
                    password,
                    None,
                );
                process.run(None, IndexMap::new());

                if !process.is_successful() {
                    return Err(Exception {
                        message: format!(
                            "Error logging in:{}",
                            self.process.get_error_output()
                        ),
                        code: 0,
                    }
                    .into());
                }
            }
        }

        Ok(())
    }

    /// @return mixed[]|null
    pub fn get_composer_information(
        &mut self,
        identifier: &str,
    ) -> Result<Option<IndexMap<String, PhpMixed>>> {
        let composer_file_content = self.get_file_content("composer.json", identifier);

        let composer_file_content = match composer_file_content {
            None => return Ok(None),
            Some(s) if s.is_empty() => return Ok(None),
            Some(s) => s,
        };

        let decoded = json_decode(&composer_file_content, true)?;
        Ok(match decoded {
            PhpMixed::Array(m) => Some(m.into_iter().map(|(k, v)| (k, *v)).collect()),
            _ => None,
        })
    }

    pub fn get_file_content(&mut self, file: &str, identifier: &str) -> Option<String> {
        let path = self.get_file_path(file, identifier)?;

        let command = self.generate_p4_command(vec!["print".to_string(), path], true);
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let result = self.command_result.clone();

        if trim(&result, None).is_empty() {
            return None;
        }

        Some(result)
    }

    pub fn get_file_path(&mut self, file: &str, identifier: &str) -> Option<String> {
        let index = strpos(identifier, "@");
        if index.is_none() {
            return Some(format!("{}/{}", identifier, file));
        }
        let idx = index.unwrap() as i64;

        let path = format!(
            "{}/{}{}",
            substr(identifier, 0, Some(idx)),
            file,
            substr(identifier, idx, None)
        );
        let command = self.generate_p4_command(vec!["files".to_string(), path], false);
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let result = self.command_result.clone();
        let index2 = strpos(&result, "no such file(s).");
        if index2.is_none() {
            let index3 = strpos(&result, "change");
            if let Some(i3) = index3 {
                let phrase = trim(&substr(&result, i3 as i64, None), None);
                let fields = explode(" ", &phrase);

                return Some(format!(
                    "{}/{}@{}",
                    substr(identifier, 0, Some(idx)),
                    file,
                    fields.get(1).cloned().unwrap_or_default()
                ));
            }
        }

        None
    }

    /// @return array{master: string}
    pub fn get_branches(&mut self) -> IndexMap<String, String> {
        let mut possible_branches: IndexMap<String, String> = IndexMap::new();
        if !self.is_stream() {
            possible_branches.insert(
                self.p4_branch.clone().unwrap_or_default(),
                self.get_stream(),
            );
        } else {
            let command = self.generate_p4_command(
                vec![
                    "streams".to_string(),
                    format!("//{}/...", self.p4_depot.as_deref().unwrap_or("")),
                ],
                true,
            );
            self.execute_command(PhpMixed::List(
                command
                    .into_iter()
                    .map(|s| Box::new(PhpMixed::String(s)))
                    .collect(),
            ));
            let result = self.command_result.clone();
            let res_array = explode(PHP_EOL, &result);
            for line in &res_array {
                let res_bits = explode(" ", line);
                if count(&PhpMixed::List(
                    res_bits
                        .iter()
                        .map(|s| Box::new(PhpMixed::String(s.clone())))
                        .collect(),
                )) > 4
                {
                    let branch = Preg::replace(
                        r"/[^A-Za-z0-9 ]/",
                        "",
                        res_bits.get(4).cloned().unwrap_or_default(),
                    );
                    possible_branches.insert(branch, res_bits.get(1).cloned().unwrap_or_default());
                }
            }
        }
        let stream = self.get_stream();
        let command = self.generate_p4_command(
            vec!["changes".to_string(), format!("{}/...", stream)],
            false,
        );
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        let last_commit = res_array.get(0).cloned().unwrap_or_default();
        let last_commit_arr = explode(" ", &last_commit);
        let last_commit_num = last_commit_arr.get(1).cloned().unwrap_or_default();

        let mut result = IndexMap::new();
        result.insert(
            "master".to_string(),
            format!(
                "{}@{}",
                possible_branches
                    .get(self.p4_branch.as_deref().unwrap_or(""))
                    .cloned()
                    .unwrap_or_default(),
                last_commit_num
            ),
        );
        result
    }

    /// @return array<string, string>
    pub fn get_tags(&mut self) -> IndexMap<String, String> {
        let command = self.generate_p4_command(vec!["labels".to_string()], true);
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        let mut tags: IndexMap<String, String> = IndexMap::new();
        let stream = self.get_stream();
        for line in &res_array {
            if strpos(line, "Label").is_some() {
                let fields = explode(" ", line);
                let key = fields.get(1).cloned().unwrap_or_default();
                tags.insert(key.clone(), format!("{}@{}", stream, key));
            }
        }

        tags
    }

    pub fn check_stream(&mut self) -> bool {
        let command = self.generate_p4_command(vec!["depots".to_string()], false);
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        for line in &res_array {
            if strpos(line, "Depot").is_some() {
                let fields = explode(" ", line);
                if strcmp(
                    self.p4_depot.as_deref().unwrap_or(""),
                    fields.get(1).map(|s| s.as_str()).unwrap_or(""),
                ) == 0
                {
                    self.p4_depot_type = Some(fields.get(3).cloned().unwrap_or_default());

                    return self.is_stream();
                }
            }
        }

        false
    }

    /// @return mixed|null
    pub(crate) fn get_change_list(&mut self, reference: &str) -> Option<String> {
        let index = strpos(reference, "@")?;
        let label = substr(reference, index as i64, None);
        let command = self.generate_p4_command(
            vec!["changes".to_string(), "-m1".to_string(), label],
            true,
        );
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));
        let changes = self.command_result.clone();
        if strpos(&changes, "Change") != Some(0) {
            return None;
        }
        let fields = explode(" ", &changes);

        Some(fields.get(1).cloned().unwrap_or_default())
    }

    /// @return mixed|null
    pub fn get_commit_logs(
        &mut self,
        from_reference: &str,
        to_reference: &str,
    ) -> Option<String> {
        let from_change_list = self.get_change_list(from_reference)?;
        let to_change_list = self.get_change_list(to_reference)?;
        let index = strpos(from_reference, "@").unwrap_or(0);
        let main = format!("{}/...", substr(from_reference, 0, Some(index as i64)));
        let command = self.generate_p4_command(
            vec![
                "filelog".to_string(),
                format!("{}@{},{}", main, from_change_list, to_change_list),
            ],
            true,
        );
        self.execute_command(PhpMixed::List(
            command
                .into_iter()
                .map(|s| Box::new(PhpMixed::String(s)))
                .collect(),
        ));

        Some(self.command_result.clone())
    }

    pub fn get_filesystem(&mut self) -> &Filesystem {
        if self.filesystem.is_none() {
            self.filesystem = Some(Filesystem::new(&self.process));
        }

        self.filesystem.as_ref().unwrap()
    }

    pub fn set_filesystem(&mut self, fs: Filesystem) {
        self.filesystem = Some(fs);
    }

    fn get_p4_executable() -> String {
        // TODO(phase-b): emulate PHP `static $p4Executable;` — cache across calls
        static P4_EXECUTABLE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        P4_EXECUTABLE
            .get_or_init(|| {
                let finder = ExecutableFinder::new();
                finder.find("p4", None, vec![]).unwrap_or_else(|| "p4".to_string())
            })
            .clone()
    }
}

