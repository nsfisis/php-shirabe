//! ref: composer/src/Composer/Util/Perforce.php

use crate::io::{IOInterface, IOInterfaceImmutable};
use crate::util::{CommandLine, Filesystem, Platform, ProcessExecutor};
use bstr::{B, BStr, BString, ByteSlice};
use indexmap::IndexMap;
use shirabe_php_shim::{
    Exception, PHP_EOL, PhpMixed, PhpResource, bconcat, chdir, date_local, explode, fclose, feof,
    fgets, file_get_contents, fopen, fwrite, gethostname, json_decode_assoc, path_from_bytes,
    php_regex, preg_replace, str_replace_array, strcmp, strlen, strpos, strrpos, substr, time,
    trim,
};
use shirabe_symfony_process::{ExecutableFinder, Process};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Perforce {
    path: PathBuf,
    p4_depot: Option<BString>,
    p4_client: Option<BString>,
    p4_user: Option<BString>,
    p4_password: Option<BString>,
    p4_port: BString,
    p4_stream: Option<BString>,
    p4_client_spec: BString,
    p4_depot_type: Option<BString>,
    p4_branch: Option<BString>,
    process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
    unique_perforce_client_name: BString,
    windows_flag: bool,
    command_result: BString,
    io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    filesystem: Option<std::rc::Rc<std::cell::RefCell<Filesystem>>>,
}

impl Perforce {
    pub fn new(
        repo_config: IndexMap<BString, PhpMixed>,
        port: BString,
        path: PathBuf,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        is_windows: bool,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> Self {
        let mut this = Self {
            path: PathBuf::new(),
            p4_depot: None,
            p4_client: None,
            p4_user: None,
            p4_password: None,
            p4_port: port,
            p4_stream: None,
            p4_client_spec: BString::default(),
            p4_depot_type: None,
            p4_branch: None,
            process,
            unique_perforce_client_name: BString::default(),
            windows_flag: is_windows,
            command_result: BString::default(),
            io,
            filesystem: None,
        };
        this.initialize_path(&path);
        this.initialize(&repo_config);
        this
    }

    pub fn create(
        repo_config: IndexMap<BString, PhpMixed>,
        port: BString,
        path: PathBuf,
        process: std::rc::Rc<std::cell::RefCell<ProcessExecutor>>,
        io: std::rc::Rc<std::cell::RefCell<dyn IOInterface>>,
    ) -> Self {
        Self::new(repo_config, port, path, process, Platform::is_windows(), io)
    }

    pub fn check_server_exists(
        url: &BStr,
        process_executor: &mut ProcessExecutor,
    ) -> anyhow::Result<bool> {
        let mut ignored_output = BString::default();
        Ok(process_executor.execute(
            &[
                BString::from("p4"),
                "-p".into(),
                url.to_owned(),
                "info".into(),
                "-s".into(),
            ],
            &mut ignored_output,
            None,
        )? == 0)
    }

    pub fn initialize(&mut self, repo_config: &IndexMap<BString, PhpMixed>) {
        self.unique_perforce_client_name = self.generate_unique_perforce_client_name();
        if repo_config.is_empty() {
            return;
        }
        if let Some(value) = repo_config
            .get(B("unique_perforce_client_name"))
            .and_then(|v| v.as_string())
        {
            self.unique_perforce_client_name = value.to_owned();
        }

        if let Some(value) = repo_config.get(B("depot")).and_then(|v| v.as_string()) {
            self.p4_depot = Some(value.to_owned());
        }
        if let Some(value) = repo_config.get(B("branch")).and_then(|v| v.as_string()) {
            self.p4_branch = Some(value.to_owned());
        }
        if let Some(value) = repo_config.get(B("p4user")).and_then(|v| v.as_string()) {
            self.p4_user = Some(value.to_owned());
        } else {
            self.p4_user = self.get_p4_variable("P4USER");
        }
        if let Some(value) = repo_config.get(B("p4password")).and_then(|v| v.as_string()) {
            self.p4_password = Some(value.to_owned());
        }
    }

    pub fn initialize_depot_and_branch(&mut self, depot: Option<&BStr>, branch: Option<&BStr>) {
        if let Some(depot) = depot {
            self.p4_depot = Some(depot.to_owned());
        }
        if let Some(branch) = branch {
            self.p4_branch = Some(branch.to_owned());
        }
    }

    pub fn generate_unique_perforce_client_name(&self) -> BString {
        bconcat!(gethostname().into_encoded_bytes(), "_", time())
    }

    pub fn cleanup_client_spec(&mut self) {
        let client = self.get_client();
        let task: Vec<BString> = vec!["client".into(), "-d".into(), client];
        let use_p4_client = false;
        let command = self.generate_p4_command(task, use_p4_client);
        self.execute_command(CommandLine::Args(command));
        let client_spec = self.get_p4_client_spec();
        let file_system = self.get_filesystem();
        file_system.borrow_mut().remove(&client_spec);
    }

    fn execute_command(&mut self, command: CommandLine) -> i64 {
        self.command_result = BString::default();

        self.process
            .borrow_mut()
            .execute(command, &mut self.command_result, None)
            .unwrap_or(1)
    }

    pub fn get_client(&mut self) -> BString {
        if self.p4_client.is_none() {
            let stream = self.get_stream();
            let clean_stream_name = str_replace_array(&["//", "/", "@"], &["", "_", ""], &stream);
            self.p4_client = Some(bconcat!(
                "composer_perforce_",
                self.unique_perforce_client_name,
                "_",
                clean_stream_name
            ));
        }

        self.p4_client.clone().unwrap_or_default()
    }

    fn get_path(&self) -> &Path {
        &self.path
    }

    pub fn initialize_path(&mut self, path: &Path) {
        self.path = path.to_path_buf();
        let fs = self.get_filesystem();
        fs.borrow_mut().ensure_directory_exists(path);
    }

    fn get_port(&self) -> &BStr {
        self.p4_port.as_bstr()
    }

    pub fn set_stream(&mut self, stream: &BStr) {
        self.p4_stream = Some(stream.to_owned());
        let index = strrpos(stream, "/");
        // Stream format is //depot/stream, while non-streaming depot is //depot
        if let Some(i) = index
            && (i as i64) > 2
        {
            self.p4_depot_type = Some("stream".into());
        }
    }

    pub fn is_stream(&self) -> bool {
        self.p4_depot_type.is_some()
            && strcmp(self.p4_depot_type.clone().unwrap_or_default(), "stream") == 0
    }

    pub fn get_stream(&mut self) -> BString {
        if self.p4_stream.is_none() {
            if self.is_stream() {
                self.p4_stream = Some(bconcat!(
                    "//",
                    self.p4_depot.clone().unwrap_or_default(),
                    "/",
                    self.p4_branch.clone().unwrap_or_default()
                ));
            } else {
                self.p4_stream = Some(bconcat!("//", self.p4_depot.clone().unwrap_or_default()));
            }
        }

        self.p4_stream.clone().unwrap_or_default()
    }

    pub fn get_stream_without_label(&self, stream: &BStr) -> BString {
        let index = strpos(stream, "@");
        match index {
            None => stream.to_owned(),
            Some(idx) => substr(stream, 0, Some(idx as i64)),
        }
    }

    pub fn get_p4_client_spec(&mut self) -> PathBuf {
        let client = self.get_client();
        path_from_bytes(&bconcat!(&self.path, "/", client, ".p4.spec")).to_path_buf()
    }

    pub fn get_user(&self) -> Option<BString> {
        self.p4_user.clone()
    }

    pub fn set_user(&mut self, user: Option<BString>) {
        self.p4_user = user;
    }

    pub fn query_p4_user(&mut self) -> anyhow::Result<()> {
        let _ = self.get_user();
        if strlen(self.p4_user.clone().unwrap_or_default()) > 0 {
            return Ok(());
        }
        self.p4_user = self.get_p4_variable("P4USER");
        if strlen(self.p4_user.clone().unwrap_or_default()) > 0 {
            return Ok(());
        }
        self.p4_user = self
            .io
            .ask("Enter P4 User:".into(), PhpMixed::Null)?
            .as_string()
            .map(BStr::to_owned);
        let command = if self.windows_flag {
            bconcat!(
                Self::get_p4_executable(),
                " set P4USER=",
                ProcessExecutor::escape(self.p4_user.clone().unwrap_or_default())
            )
        } else {
            bconcat!(
                "export P4USER=",
                ProcessExecutor::escape(self.p4_user.clone().unwrap_or_default())
            )
        };
        self.execute_command(CommandLine::Shell(command));

        Ok(())
    }

    fn get_p4_variable(&mut self, name: &str) -> Option<BString> {
        if self.windows_flag {
            let command = bconcat!(Self::get_p4_executable(), " set");
            self.execute_command(CommandLine::Shell(command));
            let result = trim(&self.command_result);
            let res_array = explode(PHP_EOL, &result);
            for line in &res_array {
                let fields = explode("=", line);
                if strcmp(
                    name,
                    fields.first().map(|s| s.as_slice()).unwrap_or_default(),
                ) == 0
                {
                    let field1 = fields.get(1).cloned().unwrap_or_default();
                    let index = strpos(&field1, " ");
                    let value = match index {
                        None => field1,
                        Some(idx) => substr(&field1, 0, Some(idx as i64)),
                    };

                    return Some(trim(&value));
                }
            }

            return None;
        }

        let command = bconcat!("echo $", name);
        self.execute_command(CommandLine::Shell(command));
        let result = trim(&self.command_result);

        Some(result)
    }

    pub fn query_p4_password(&mut self) -> anyhow::Result<Option<BString>> {
        if let Some(ref p) = self.p4_password {
            return Ok(Some(p.clone()));
        }
        let mut password = self.get_p4_variable("P4PASSWD");
        if strlen(password.clone().unwrap_or_default()) <= 0 {
            password = self
                .io
                .ask_and_hide_answer(format!(
                    "Enter password for Perforce user {}: ",
                    self.get_user().unwrap_or_default().display_lossy()
                ))?
                .map(BString::from);
        }
        self.p4_password = password.clone();

        Ok(password)
    }

    pub fn generate_p4_command(
        &mut self,
        arguments: Vec<BString>,
        use_client: bool,
    ) -> Vec<BString> {
        let mut p4_command: Vec<BString> = vec![Self::get_p4_executable()];
        if self.get_user().is_some() {
            p4_command.push("-u".into());
            p4_command.push(self.get_user().unwrap_or_default());
        }
        if use_client {
            p4_command.push("-c".into());
            p4_command.push(self.get_client());
        }
        p4_command.push("-p".into());
        p4_command.push(self.get_port().to_owned());

        let mut result = p4_command;
        result.extend(arguments);
        result
    }

    pub fn is_logged_in(&mut self) -> anyhow::Result<bool> {
        let command = self.generate_p4_command(vec!["login".into(), "-s".into()], false);
        let exit_code = self.execute_command(CommandLine::Args(command));
        if exit_code != 0 {
            let error_output = self.process.borrow().get_error_output();
            let user = self.get_user().unwrap_or_default();
            let index = strpos(&error_output, &user);
            if index.is_none() {
                let index = strpos(&error_output, "p4");
                if index.is_none() {
                    return Ok(false);
                }
                return Err(Exception::new(format!(
                    "p4 command not found in path: {}",
                    error_output.display_lossy()
                ))
                .into());
            }
            return Err(
                Exception::new(format!("Invalid user name: {}", user.display_lossy())).into(),
            );
        }

        Ok(true)
    }

    pub fn connect_client(&mut self) -> anyhow::Result<()> {
        let p4_create_client_command =
            self.generate_p4_command(vec!["client".into(), "-i".into()], true);

        let mut process = Process::new(
            p4_create_client_command,
            None,
            None,
            file_get_contents(self.get_p4_client_spec())
                .map(|s| PhpMixed::String(s.into()))
                .unwrap_or(PhpMixed::Null),
            None,
        )?;
        process.run(None, indexmap::IndexMap::new())?;
        Ok(())
    }

    pub fn sync_code_base(&mut self, source_reference: Option<&BStr>) -> anyhow::Result<()> {
        let prev_dir = Platform::get_cwd(false)?;
        chdir(&self.path);
        let mut p4_sync_command = self.generate_p4_command(vec!["sync".into(), "-f".into()], true);
        if let Some(source_reference) = source_reference {
            p4_sync_command.push(bconcat!("@", source_reference));
        }
        self.execute_command(CommandLine::Args(p4_sync_command));
        chdir(&prev_dir);

        Ok(())
    }

    pub fn write_client_spec_to_file(&mut self, spec: &PhpResource) {
        let client = self.get_client();
        fwrite(spec, bconcat!("Client: ", client, PHP_EOL, PHP_EOL), None);
        fwrite(
            spec,
            bconcat!(
                "Update: ",
                date_local("Y/m/d H:i:s", None),
                PHP_EOL,
                PHP_EOL
            ),
            None,
        );
        fwrite(
            spec,
            bconcat!("Access: ", date_local("Y/m/d H:i:s", None), PHP_EOL),
            None,
        );
        fwrite(
            spec,
            bconcat!(
                "Owner:  ",
                self.get_user().unwrap_or_default(),
                PHP_EOL,
                PHP_EOL
            ),
            None,
        );
        fwrite(spec, bconcat!("Description:", PHP_EOL), None);
        fwrite(
            spec,
            bconcat!(
                "  Created by ",
                self.get_user().unwrap_or_default(),
                " from composer.",
                PHP_EOL,
                PHP_EOL
            ),
            None,
        );
        fwrite(
            spec,
            bconcat!("Root: ", self.get_path(), PHP_EOL, PHP_EOL),
            None,
        );
        fwrite(
            spec,
            bconcat!(
                "Options:  noallwrite noclobber nocompress unlocked modtime rmdir",
                PHP_EOL,
                PHP_EOL
            ),
            None,
        );
        fwrite(
            spec,
            bconcat!("SubmitOptions:  revertunchanged", PHP_EOL, PHP_EOL),
            None,
        );
        fwrite(spec, bconcat!("LineEnd:  local", PHP_EOL, PHP_EOL), None);
        if self.is_stream() {
            fwrite(spec, bconcat!("Stream:", PHP_EOL), None);
            let stream_clone = self.p4_stream.clone().unwrap_or_default();
            fwrite(
                spec,
                bconcat!(
                    "  ",
                    self.get_stream_without_label(stream_clone.as_bstr()),
                    PHP_EOL
                ),
                None,
            );
        } else {
            let stream = self.get_stream();
            let client = self.get_client();
            fwrite(
                spec,
                bconcat!("View:  ", stream, "/...  //", client, "/... ", PHP_EOL),
                None,
            );
        }
    }

    pub fn write_p4_client_spec(&mut self) -> anyhow::Result<()> {
        let client_spec = self.get_p4_client_spec();
        let spec = fopen(&client_spec, "w")?;
        self.write_client_spec_to_file(&spec);
        fclose(&spec);
        Ok(())
    }

    fn read(&self, pipe: &PhpResource, _name: PhpMixed) {
        if feof(pipe) {
            return;
        }
        let mut line = fgets(pipe, None);
        while line.is_some() {
            line = fgets(pipe, None);
        }
    }

    pub fn windows_login(&mut self, password: Option<&BStr>) -> anyhow::Result<i64> {
        let command = self.generate_p4_command(vec!["login".into(), "-a".into()], true);

        let mut process = Process::new(
            command,
            None,
            None,
            password
                .map(|s| PhpMixed::string(s.to_owned()))
                .unwrap_or(PhpMixed::Null),
            None,
        )?;

        process.run(None, indexmap::IndexMap::new())
    }

    pub fn p4_login(&mut self) -> anyhow::Result<()> {
        self.query_p4_user()?;
        if !self.is_logged_in()? {
            let password = self.query_p4_password()?;
            if self.windows_flag {
                self.windows_login(password.as_ref().map(|p| p.as_bstr()))?;
            } else {
                let command = self.generate_p4_command(vec!["login".into(), "-a".into()], false);

                let mut process = Process::new(
                    command,
                    None,
                    None,
                    password.map(PhpMixed::string).unwrap_or(PhpMixed::Null),
                    None,
                )?;
                process.run(None, indexmap::IndexMap::new())?;

                if !process.is_successful() {
                    return Err(Exception::new(format!(
                        "Error logging in:{}",
                        self.process.borrow().get_error_output().display_lossy()
                    ))
                    .into());
                }
            }
        }

        Ok(())
    }

    pub fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        let composer_file_content = self.get_file_content(B("composer.json").as_bstr(), identifier);

        let composer_file_content = match composer_file_content {
            None => return Ok(None),
            Some(s) if s.is_empty() => return Ok(None),
            Some(s) => s,
        };

        let decoded = json_decode_assoc(&composer_file_content)?;
        Ok(match decoded {
            PhpMixed::Array(m) => Some(m.into_iter().collect()),
            _ => None,
        })
    }

    pub fn get_file_content(&mut self, file: &BStr, identifier: &BStr) -> Option<BString> {
        let path = self.get_file_path(file, identifier)?;

        let command = self.generate_p4_command(vec!["print".into(), path], true);
        self.execute_command(CommandLine::Args(command));
        let result = self.command_result.clone();

        if trim(&result).is_empty() {
            return None;
        }

        Some(result)
    }

    pub fn get_file_path(&mut self, file: &BStr, identifier: &BStr) -> Option<BString> {
        let index = strpos(identifier, "@");
        if index.is_none() {
            return Some(bconcat!(identifier, "/", file));
        }
        let idx = index.unwrap() as i64;

        let path = bconcat!(
            substr(identifier, 0, Some(idx)),
            "/",
            file,
            substr(identifier, idx, None)
        );
        let command = self.generate_p4_command(vec!["files".into(), path], false);
        self.execute_command(CommandLine::Args(command));
        let result = self.command_result.clone();
        let index2 = strpos(&result, "no such file(s).");
        if index2.is_none() {
            let index3 = strpos(&result, "change");
            if let Some(i3) = index3 {
                let phrase = trim(substr(&result, i3 as i64, None));
                let fields = explode(" ", &phrase);

                return Some(bconcat!(
                    substr(identifier, 0, Some(idx)),
                    "/",
                    file,
                    "@",
                    fields.get(1).cloned().unwrap_or_default()
                ));
            }
        }

        None
    }

    pub fn get_branches(&mut self) -> IndexMap<BString, BString> {
        let mut possible_branches: IndexMap<BString, BString> = IndexMap::new();
        if !self.is_stream() {
            let stream = self.get_stream();
            possible_branches.insert(self.p4_branch.clone().unwrap_or_default(), stream);
        } else {
            let command = self.generate_p4_command(
                vec![
                    "streams".into(),
                    bconcat!("//", self.p4_depot.clone().unwrap_or_default(), "/..."),
                ],
                true,
            );
            self.execute_command(CommandLine::Args(command));
            let result = self.command_result.clone();
            let res_array = explode(PHP_EOL, &result);
            for line in &res_array {
                let res_bits = explode(" ", line);
                if res_bits.len() > 4 {
                    let branch = preg_replace(
                        php_regex!(r"/[^A-Za-z0-9 ]/"),
                        "",
                        res_bits.get(4).cloned().unwrap_or_default(),
                    );
                    possible_branches.insert(branch, res_bits.get(1).cloned().unwrap_or_default());
                }
            }
        }
        let stream = self.get_stream();
        let command =
            self.generate_p4_command(vec!["changes".into(), bconcat!(stream, "/...")], false);
        self.execute_command(CommandLine::Args(command));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        let last_commit = res_array.first().cloned().unwrap_or_default();
        let last_commit_arr = explode(" ", &last_commit);
        let last_commit_num = last_commit_arr.get(1).cloned().unwrap_or_default();

        let mut result = IndexMap::new();
        result.insert(
            "master".into(),
            bconcat!(
                possible_branches
                    .get(self.p4_branch.clone().unwrap_or_default().as_slice())
                    .cloned()
                    .unwrap_or_default(),
                "@",
                last_commit_num
            ),
        );
        result
    }

    pub fn get_tags(&mut self) -> IndexMap<BString, BString> {
        let command = self.generate_p4_command(vec!["labels".into()], true);
        self.execute_command(CommandLine::Args(command));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        let mut tags: IndexMap<BString, BString> = IndexMap::new();
        let stream = self.get_stream();
        for line in &res_array {
            if strpos(line, "Label").is_some() {
                let fields = explode(" ", line);
                let key = fields.get(1).cloned().unwrap_or_default();
                tags.insert(key.clone(), bconcat!(stream, "@", key));
            }
        }

        tags
    }

    pub fn check_stream(&mut self) -> bool {
        let command = self.generate_p4_command(vec!["depots".into()], false);
        self.execute_command(CommandLine::Args(command));
        let result = self.command_result.clone();
        let res_array = explode(PHP_EOL, &result);
        for line in &res_array {
            if strpos(line, "Depot").is_some() {
                let fields = explode(" ", line);
                if strcmp(
                    self.p4_depot.clone().unwrap_or_default(),
                    fields.get(1).map(|s| s.as_slice()).unwrap_or_default(),
                ) == 0
                {
                    self.p4_depot_type = Some(fields.get(3).cloned().unwrap_or_default());

                    return self.is_stream();
                }
            }
        }

        false
    }

    fn get_change_list(&mut self, reference: &BStr) -> Option<BString> {
        let index = strpos(reference, "@")?;
        let label = substr(reference, index as i64, None);
        let command = self.generate_p4_command(vec!["changes".into(), "-m1".into(), label], true);
        self.execute_command(CommandLine::Args(command));
        let changes = self.command_result.clone();
        if strpos(&changes, "Change") != Some(0) {
            return None;
        }
        let fields = explode(" ", &changes);

        Some(fields.get(1).cloned().unwrap_or_default())
    }

    pub fn get_commit_logs(
        &mut self,
        from_reference: &BStr,
        to_reference: &BStr,
    ) -> Option<BString> {
        let from_change_list = self.get_change_list(from_reference)?;
        let to_change_list = self.get_change_list(to_reference)?;
        let index = strpos(from_reference, "@").unwrap_or(0);
        let main = bconcat!(substr(from_reference, 0, Some(index as i64)), "/...");
        let command = self.generate_p4_command(
            vec![
                "filelog".into(),
                bconcat!(main, "@", from_change_list, ",", to_change_list),
            ],
            true,
        );
        self.execute_command(CommandLine::Args(command));

        Some(self.command_result.clone())
    }

    pub fn get_filesystem(&mut self) -> &std::rc::Rc<std::cell::RefCell<Filesystem>> {
        if self.filesystem.is_none() {
            self.filesystem = Some(std::rc::Rc::new(std::cell::RefCell::new(Filesystem::new(
                Some(self.process.clone()),
            ))));
        }

        self.filesystem.as_ref().unwrap()
    }

    pub fn set_filesystem(&mut self, fs: std::rc::Rc<std::cell::RefCell<Filesystem>>) {
        self.filesystem = Some(fs);
    }

    fn get_p4_executable() -> BString {
        static P4_EXECUTABLE: std::sync::OnceLock<BString> = std::sync::OnceLock::new();

        P4_EXECUTABLE
            .get_or_init(|| {
                let finder = ExecutableFinder::new();
                BString::from(finder.find("p4", None, &[]).unwrap_or("p4".to_string()))
            })
            .clone()
    }
}

/// Seam over `Perforce` so consumers can be exercised with a mock in tests.
pub trait PerforceInterface: std::fmt::Debug {
    fn initialize_path(&mut self, path: &Path);
    fn set_stream(&mut self, stream: &BStr);
    fn p4_login(&mut self) -> anyhow::Result<()>;
    fn check_stream(&mut self) -> bool;
    fn write_p4_client_spec(&mut self) -> anyhow::Result<()>;
    fn connect_client(&mut self) -> anyhow::Result<()>;
    fn sync_code_base(&mut self, source_reference: Option<BString>) -> anyhow::Result<()>;
    fn cleanup_client_spec(&mut self);
    fn get_commit_logs(&mut self, from_reference: &BStr, to_reference: &BStr) -> Option<BString>;
    fn get_file_content(&mut self, file: &BStr, identifier: &BStr) -> Option<BString>;
    fn get_branches(&mut self) -> IndexMap<BString, BString>;
    fn get_tags(&mut self) -> IndexMap<BString, BString>;
    fn get_user(&self) -> Option<BString>;
    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>>;
}

impl PerforceInterface for Perforce {
    fn initialize_path(&mut self, path: &Path) {
        Perforce::initialize_path(self, path)
    }

    fn set_stream(&mut self, stream: &BStr) {
        Perforce::set_stream(self, stream)
    }

    fn p4_login(&mut self) -> anyhow::Result<()> {
        Perforce::p4_login(self)
    }

    fn check_stream(&mut self) -> bool {
        Perforce::check_stream(self)
    }

    fn write_p4_client_spec(&mut self) -> anyhow::Result<()> {
        Perforce::write_p4_client_spec(self)
    }

    fn connect_client(&mut self) -> anyhow::Result<()> {
        Perforce::connect_client(self)
    }

    fn sync_code_base(&mut self, source_reference: Option<BString>) -> anyhow::Result<()> {
        Perforce::sync_code_base(self, source_reference.as_ref().map(|r| r.as_bstr()))
    }

    fn cleanup_client_spec(&mut self) {
        Perforce::cleanup_client_spec(self)
    }

    fn get_commit_logs(&mut self, from_reference: &BStr, to_reference: &BStr) -> Option<BString> {
        Perforce::get_commit_logs(self, from_reference, to_reference)
    }

    fn get_file_content(&mut self, file: &BStr, identifier: &BStr) -> Option<BString> {
        Perforce::get_file_content(self, file, identifier)
    }

    fn get_branches(&mut self) -> IndexMap<BString, BString> {
        Perforce::get_branches(self)
    }

    fn get_tags(&mut self) -> IndexMap<BString, BString> {
        Perforce::get_tags(self)
    }

    fn get_user(&self) -> Option<BString> {
        Perforce::get_user(self)
    }

    fn get_composer_information(
        &mut self,
        identifier: &BStr,
    ) -> anyhow::Result<Option<IndexMap<BString, PhpMixed>>> {
        Perforce::get_composer_information(self, identifier)
    }
}
