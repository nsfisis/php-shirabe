//! ref: composer/tests/Composer/Test/Question/StrictConfirmationQuestionTest.php

use shirabe::question::StrictConfirmationQuestion;
use shirabe_external_packages::symfony::console::helper::question_helper::QuestionHelper;
use shirabe_external_packages::symfony::console::input::array_input::ArrayInput;
use shirabe_external_packages::symfony::console::input::streamable_input_interface::StreamableInputInterface;
use shirabe_external_packages::symfony::console::output::output_interface::OutputInterface;
use shirabe_external_packages::symfony::console::output::stream_output::StreamOutput;
use shirabe_php_shim::PhpMixed;
use std::cell::RefCell;
use std::rc::Rc;

const TRUE_ANSWER_REGEX: &str = "/^y(?:es)?$/i";
const FALSE_ANSWER_REGEX: &str = "/^no?$/i";

/// @return string[][]
fn get_ask_confirmation_bad_data() -> Vec<&'static str> {
    vec!["not correct", "no more", "yes please", "yellow"]
}

#[test]
fn test_ask_confirmation_bad_answer() {
    for answer in get_ask_confirmation_bad_data() {
        let (mut input, mut dialog) = create_input(&format!("{}\n", answer));

        let mut question = StrictConfirmationQuestion::new(
            "Do you like French fries?".to_string(),
            true,
            TRUE_ANSWER_REGEX.to_string(),
            FALSE_ANSWER_REGEX.to_string(),
        );
        question.inner_mut().set_max_attempts(Some(1)).unwrap();

        let error = dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .expect_err("expected an InvalidArgumentException");
        assert_eq!(error.to_string(), "Please answer yes, y, no, or n.");
    }
}

#[test]
fn test_ask_confirmation() {
    for (question, expected, default) in get_ask_confirmation_data() {
        let (mut input, mut dialog) = create_input(&format!("{}\n", question));

        let question = StrictConfirmationQuestion::new(
            "Do you like French fries?".to_string(),
            default,
            TRUE_ANSWER_REGEX.to_string(),
            FALSE_ANSWER_REGEX.to_string(),
        );
        assert_eq!(
            dialog
                .ask(&mut input, create_output_interface(), question.inner())
                .unwrap()
                .unwrap(),
            PhpMixed::Bool(expected),
            "confirmation question should {}",
            if expected { "pass" } else { "cancel" }
        );
    }
}

/// @return mixed[][]
fn get_ask_confirmation_data() -> Vec<(&'static str, bool, bool)> {
    vec![
        ("", true, true),
        ("", false, false),
        ("y", true, true),
        ("yes", true, true),
        ("n", false, true),
        ("no", false, true),
    ]
}

#[test]
fn test_ask_confirmation_with_custom_true_and_false_answer() {
    let question = StrictConfirmationQuestion::new(
        "Do you like French fries?".to_string(),
        false,
        "/^ja$/i".to_string(),
        "/^nein$/i".to_string(),
    );

    let (mut input, mut dialog) = create_input("ja\n");
    assert_eq!(
        dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .unwrap()
            .unwrap(),
        PhpMixed::Bool(true)
    );

    let (mut input, mut dialog) = create_input("nein\n");
    assert_eq!(
        dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .unwrap()
            .unwrap(),
        PhpMixed::Bool(false)
    );
}

/// @return resource
fn get_input_stream(input: &str) -> shirabe_php_shim::PhpResource {
    let stream = shirabe_php_shim::php_fopen_resource("php://memory", "r+");

    shirabe_php_shim::fwrite_resource(&stream, input);
    shirabe_php_shim::rewind(&stream);

    stream
}

fn create_output_interface() -> Rc<RefCell<dyn OutputInterface>> {
    let stream = shirabe_php_shim::php_fopen_resource("php://memory", "r+");
    let output = StreamOutput::new(stream, None, None, None)
        .unwrap()
        .expect("php://memory is a valid stream");
    Rc::new(RefCell::new(output))
}

/// @return array{ArrayInput, QuestionHelper}
fn create_input(entry: &str) -> (ArrayInput, QuestionHelper) {
    let mut input = ArrayInput::new(
        vec![(
            PhpMixed::Int(0),
            PhpMixed::String("--no-interaction".to_string()),
        )],
        None,
    )
    .unwrap();
    input.set_stream(get_input_stream(entry));

    let dialog = QuestionHelper::default();

    (input, dialog)
}
