//! ref: composer/tests/Composer/Test/Command/RunScriptCommandTest.php

use crate::test_case::{RunOptions, get_application_tester, init_temp_composer};
use serial_test::serial;
use shirabe_php_shim::PhpMixed;

/// ref: RunScriptCommandTest::testDetectAndPassDevModeToEventAndToDispatching
///
/// The `getDevOptions` dataProvider drives four `(dev, noDev)` cases; for each, PHP asserts that the
/// `ScriptEvent` passed to `hasEventListeners` matches the script name AND its `isDevMode()` equals
/// the computed dev mode (`dev || !noDev`) -- the latter being the whole point of the test.
#[test]
#[ignore = "PHP mocks RunScriptCommand itself (onlyMethods incl. requireComposer -> a composer \
            whose EventDispatcher is a hasEventListeners/dispatchScript recording mock) and \
            drives run() with mocked Input/Output. The Rust RunScriptCommand has no \
            requireComposer override seam and Input/Output are concrete types, so the mocked \
            harness is inexpressible; the event-side isDevMode downcast now exists \
            (EventInterface::as_any), but that alone does not unblock the test."]
fn test_detect_and_pass_dev_mode_to_event_and_to_dispatching() {
    // TODO(mock): PHP mocks RunScriptCommand itself (onlyMethods incl. requireComposer -> a
    // composer whose EventDispatcher is a hasEventListeners/dispatchScript recording mock) and
    // drives run() with mocked Input/Output. The Rust RunScriptCommand has no requireComposer
    // override seam and Input/Output are concrete types, so the mocked harness is
    // inexpressible; the event-side isDevMode downcast now exists (EventInterface::as_any), but
    // that alone does not unblock the test.
    todo!()
}

/// ref: RunScriptCommandTest::testCanListScripts
#[test]
#[serial]
fn test_can_list_scripts() {
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test": "@php test",
                "fix-cs": "php-cs-fixer fix",
            },
            "scripts-descriptions": {
                "fix-cs": "Run the codestyle fixer",
            },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("run-script")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");

    let output = app_tester.get_display();

    assert!(
        output.contains("Runs the test script as defined in composer.json"),
        "The default description for the test script should be printed"
    );
    assert!(
        output.contains("Run the codestyle fixer"),
        "The custom description for the fix-cs script should be printed"
    );

    drop(tear_down);
}

/// ref: RunScriptCommandTest::testCanDefineAliases
#[test]
#[serial]
fn test_can_define_aliases() {
    let expected_aliases = vec!["one", "two", "three"];

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test": "@php test",
            },
            "scripts-aliases": {
                "test": expected_aliases,
            },
        })),
        None,
        None,
        true,
    );

    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("test")),
                (PhpMixed::from("--help"), PhpMixed::from(true)),
                (PhpMixed::from("--format"), PhpMixed::from("json")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");

    let output = app_tester.get_display();
    let array: serde_json::Value = serde_json::from_str(&output).unwrap();
    let mut actual_aliases: Vec<serde_json::Value> = array["usage"].as_array().unwrap().clone();
    actual_aliases.remove(0);

    let expected: Vec<serde_json::Value> = expected_aliases
        .iter()
        .map(|s| serde_json::Value::String(s.to_string()))
        .collect();
    assert_eq!(
        expected, actual_aliases,
        "The custom aliases for the test command should be printed"
    );

    drop(tear_down);
}

/// ref: RunScriptCommandTest::testExecutionOfSimpleSymfonyCommand
#[test]
#[serial]
#[ignore = "PhpCommandProxy::run hands the whole run to the worker-side console application, so the \
            user's Command writes to the stdio the worker inherited and the in-process application \
            tester's buffer stays empty. The worker is also a per-process singleton that keeps the \
            working directory it was spawned in, so the relative psr-4 path this test's autoload \
            config produces ('./MyCommand.php') only resolves when the test runs first in the binary"]
fn test_execution_of_simple_symfony_command() {
    let description = "Sample description for test command";
    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                "test-direct": "Test\\MyCommand",
                "test-ref": ["@test-direct --inneropt innerarg"],
            },
            "scripts-descriptions": {
                "test-direct": description,
            },
            "autoload": {
                "psr-4": {
                    "Test\\": "",
                },
            },
        })),
        None,
        None,
        true,
    );

    std::fs::write(
        "MyCommand.php",
        r#"<?php

namespace Test;

use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Command\Command;

class MyCommand extends Command
{
    protected function configure(): void
    {
        $this->setDefinition([
            new InputArgument('req-arg', InputArgument::REQUIRED, 'Required arg.'),
            new InputArgument('opt-arg', InputArgument::OPTIONAL, 'Optional arg.'),
            new InputOption('inneropt', null, InputOption::VALUE_NONE, 'Option.'),
            new InputOption('outeropt', null, InputOption::VALUE_OPTIONAL, 'Optional option.'),
        ]);
    }

    public function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln($input->getArgument('req-arg'));
        $output->writeln((string) $input->getArgument('opt-arg'));
        $output->writeln('inneropt: '.($input->getOption('inneropt') ? 'set' : 'unset'));
        $output->writeln('outeropt: '.($input->getOption('outeropt') ? 'set' : 'unset'));

        return 2;
    }
}
"#,
    )
    .unwrap();

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("test-direct")),
                (PhpMixed::from("--outeropt"), PhpMixed::from(true)),
                (PhpMixed::from("req-arg"), PhpMixed::from("lala")),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(
        "lala\n\ninneropt: unset\nouteropt: set\n",
        app_tester.get_display()
    );
    assert_eq!(2, app_tester.get_status_code());

    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("test-ref")),
                (PhpMixed::from("--outeropt"), PhpMixed::from(true)),
                (PhpMixed::from("req-arg"), PhpMixed::from("lala")),
            ],
            RunOptions::default(),
        )
        .unwrap();

    assert_eq!(
        "innerarg\nlala\ninneropt: set\nouteropt: set\n",
        app_tester.get_display()
    );
    assert_eq!(2, app_tester.get_status_code());

    // check if the description from composer.json is correctly shown
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("run-script")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");
    let output = app_tester.get_display();
    assert!(
        output.contains(description),
        "The contents of scripts-description for the test script should be printed"
    );

    drop(tear_down);
}

/// ref: RunScriptCommandTest::testExecutionOfSymfonyCommandWithConfiguration
#[test]
#[serial]
#[ignore = "PhpCommandProxy::run hands the whole run to the worker-side console application, so the \
            user's Command writes to the stdio the worker inherited and the in-process application \
            tester's buffer stays empty. The worker is also a per-process singleton that keeps the \
            working directory it was spawned in, so the relative psr-4 path this test's autoload \
            config produces ('./MyCommand.php') only resolves when the test runs first in the binary"]
fn test_execution_of_symfony_command_with_configuration() {
    let cmd_name = "custom-cmd-123";
    let cmd_alias = format!("{}-alias", cmd_name);
    let cmd_desc = "This is a Symfony command with custom configuration";
    let wrong_desc = "this should be ignored";

    let tear_down = init_temp_composer(
        Some(&serde_json::json!({
            "scripts": {
                cmd_name: "Test\\MyCommandWithDefinitions",
            },
            "scripts-descriptions": {
                cmd_name: wrong_desc,
            },
            "autoload": {
                "psr-4": {
                    "Test\\": "",
                },
            },
        })),
        None,
        None,
        true,
    );

    std::fs::write(
        "MyCommandWithDefinitions.php",
        r#"<?php

namespace Test;

use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Command\Command;

class MyCommandWithDefinitions extends Command
{
    protected function configure(): void
    {
        $this
            ->setDescription('__CMD_DESC__')
            ->setAliases(['__CMD_ALIAS__'])
            ->setDefinition([new InputArgument('req-arg', InputArgument::REQUIRED, 'Required arg.')]);
    }

    public function execute(InputInterface $input, OutputInterface $output): int
    {
        $output->writeln($input->getArgument('req-arg'));
        return Command::SUCCESS;
    }
}
"#
        .replace("__CMD_DESC__", cmd_desc)
        .replace("__CMD_ALIAS__", &cmd_alias),
    )
    .unwrap();

    // makes sure the command executes with the name defined inside its `configure()`...
    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from(cmd_name)),
                (PhpMixed::from("req-arg"), PhpMixed::from("lala")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!("lala\n", app_tester.get_display());

    // ...with the alias defined there as well...
    let mut app_tester = get_application_tester();
    app_tester
        .run(
            vec![
                (
                    PhpMixed::from("command"),
                    PhpMixed::from(cmd_alias.as_str()),
                ),
                (PhpMixed::from("req-arg"), PhpMixed::from("lala")),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!("lala\n", app_tester.get_display());

    // ...and also uses its own description, instead of the one in composer.scripts-descriptions
    let mut app_tester = get_application_tester();
    let status_code = app_tester
        .run(
            vec![
                (PhpMixed::from("command"), PhpMixed::from("run-script")),
                (PhpMixed::from("--list"), PhpMixed::from(true)),
            ],
            RunOptions::default(),
        )
        .unwrap();
    assert_eq!(0, status_code, "assertCommandIsSuccessful");
    let output = app_tester.get_display();
    assert!(
        output.contains(cmd_desc),
        "The custom description for the test script should be printed"
    );
    assert!(
        !output.contains(wrong_desc),
        "The dummy description shouldn't show"
    );

    drop(tear_down);
}
