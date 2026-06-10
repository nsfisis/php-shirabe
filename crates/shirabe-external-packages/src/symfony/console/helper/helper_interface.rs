use crate::symfony::console::helper::HelperSet;

pub trait HelperInterface: std::fmt::Debug {
    fn set_helper_set(&mut self, _helper_set: Option<std::rc::Rc<std::cell::RefCell<HelperSet>>>) {
        todo!()
    }

    fn get_helper_set(&self) -> Option<std::rc::Rc<std::cell::RefCell<HelperSet>>> {
        todo!()
    }

    fn get_name(&self) -> String {
        todo!()
    }

    fn as_any(&self) -> &dyn std::any::Any;
}
