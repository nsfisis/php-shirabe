pub trait LoggerInterface {
    fn emergency(&self, message: &str, context: &[(&str, &str)]);
    fn alert(&self, message: &str, context: &[(&str, &str)]);
    fn critical(&self, message: &str, context: &[(&str, &str)]);
    fn error(&self, message: &str, context: &[(&str, &str)]);
    fn warning(&self, message: &str, context: &[(&str, &str)]);
    fn notice(&self, message: &str, context: &[(&str, &str)]);
    fn info(&self, message: &str, context: &[(&str, &str)]);
    fn debug(&self, message: &str, context: &[(&str, &str)]);
    fn log(&self, level: &str, message: &str, context: &[(&str, &str)]);
}
