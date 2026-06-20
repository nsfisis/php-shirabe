//! ref: composer/tests/Composer/Test/Question/StrictConfirmationQuestionTest.php

// These drive StrictConfirmationQuestion through Symfony's QuestionHelper::ask using
// ArrayInput/StreamOutput, none of which are ported. The question's normalizer and
// validator are private, so they cannot be exercised directly either.

#[test]
#[ignore = "needs Symfony QuestionHelper::ask with ArrayInput/StreamOutput, which are not ported"]
fn test_ask_confirmation_bad_answer() {
    todo!()
}

#[test]
#[ignore = "needs Symfony QuestionHelper::ask with ArrayInput/StreamOutput, which are not ported"]
fn test_ask_confirmation() {
    todo!()
}

#[test]
#[ignore = "needs Symfony QuestionHelper::ask with ArrayInput/StreamOutput, which are not ported"]
fn test_ask_confirmation_with_custom_true_and_false_answer() {
    todo!()
}
