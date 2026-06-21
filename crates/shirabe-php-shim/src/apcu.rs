use crate::PhpMixed;

pub fn apcu_add(key: &str, var: PhpMixed) -> bool {
    let _ = (key, var);
    todo!()
}

pub fn apcu_fetch(key: &str, success: &mut bool) -> PhpMixed {
    let _ = (key, success);
    todo!()
}
