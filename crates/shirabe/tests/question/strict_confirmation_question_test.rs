//! ref: composer/tests/Composer/Test/Question/StrictConfirmationQuestionTest.php

use bstr::{BStr, BString, ByteSlice};
use shirabe::question::StrictConfirmationQuestion;
use shirabe_php_shim::{
    PhpResource, bconcat, error_message, fwrite_resource, php_fopen_resource, php_str, rewind,
};
use shirabe_symfony_console::helper::{QuestionHelper, QuestionHelperInterface};
use shirabe_symfony_console::input::{
    ArrayInput, InputValue, ParameterName, StreamableInputInterface,
};
use shirabe_symfony_console::output::{OutputInterface, StreamOutput};

const TRUE_ANSWER_REGEX: &BStr = php_str(b"/^y(?:es)?$/i");
const FALSE_ANSWER_REGEX: &BStr = php_str(b"/^no?$/i");

fn get_ask_confirmation_bad_data() -> Vec<&'static BStr> {
    vec![
        BStr::new("not correct"),
        BStr::new("no more"),
        BStr::new("yes please"),
        BStr::new("yellow"),
    ]
}

#[test]
fn test_ask_confirmation_bad_answer() {
    for answer in get_ask_confirmation_bad_data() {
        let (mut input, mut dialog) = create_input(bconcat!(answer, "\n").as_bstr());

        let mut question = StrictConfirmationQuestion::new(
            BString::from("Do you like French fries?"),
            true,
            bconcat!(TRUE_ANSWER_REGEX),
            bconcat!(FALSE_ANSWER_REGEX),
        );
        question.inner_mut().set_max_attempts(Some(1)).unwrap();

        let error = dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .expect_err("expected an InvalidArgumentException");
        assert_eq!(error_message(&error), "Please answer yes, y, no, or n.");
    }
}

#[test]
fn test_ask_confirmation() {
    for (question, expected, default) in get_ask_confirmation_data() {
        let (mut input, mut dialog) = create_input(bconcat!(question, "\n").as_bstr());

        let question = StrictConfirmationQuestion::new(
            BString::from("Do you like French fries?"),
            default,
            bconcat!(TRUE_ANSWER_REGEX),
            bconcat!(FALSE_ANSWER_REGEX),
        );
        assert_eq!(
            dialog
                .ask(&mut input, create_output_interface(), question.inner())
                .unwrap()
                .unwrap(),
            InputValue::Bool(expected),
            "confirmation question should {}",
            if expected { "pass" } else { "cancel" }
        );
    }
}

fn get_ask_confirmation_data() -> Vec<(&'static BStr, bool, bool)> {
    vec![
        (BStr::new(""), true, true),
        (BStr::new(""), false, false),
        (BStr::new("y"), true, true),
        (BStr::new("yes"), true, true),
        (BStr::new("n"), false, true),
        (BStr::new("no"), false, true),
    ]
}

#[test]
fn test_ask_confirmation_with_custom_true_and_false_answer() {
    let question = StrictConfirmationQuestion::new(
        BString::from("Do you like French fries?"),
        false,
        BString::from("/^ja$/i"),
        BString::from("/^nein$/i"),
    );

    let (mut input, mut dialog) = create_input(BStr::new("ja\n"));
    assert_eq!(
        dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .unwrap()
            .unwrap(),
        InputValue::Bool(true)
    );

    let (mut input, mut dialog) = create_input(BStr::new("nein\n"));
    assert_eq!(
        dialog
            .ask(&mut input, create_output_interface(), question.inner())
            .unwrap()
            .unwrap(),
        InputValue::Bool(false)
    );
}

fn get_input_stream(input: &BStr) -> PhpResource {
    let stream = php_fopen_resource("php://memory", "r+");

    fwrite_resource(&stream, input);
    rewind(&stream);

    stream
}

fn create_output_interface() -> std::rc::Rc<std::cell::RefCell<dyn OutputInterface>> {
    let stream = php_fopen_resource("php://memory", "r+");
    let output = StreamOutput::new(stream, None, None, None)
        .unwrap()
        .expect("php://memory is a valid stream");
    std::rc::Rc::new(std::cell::RefCell::new(output))
}

fn create_input(entry: &BStr) -> (ArrayInput, QuestionHelper) {
    let mut input = ArrayInput::new(
        vec![(
            ParameterName::Index(0),
            InputValue::String(BString::from("--no-interaction")),
        )],
        None,
    )
    .unwrap();
    input.set_stream(get_input_stream(entry));

    let dialog = QuestionHelper::default();

    (input, dialog)
}
