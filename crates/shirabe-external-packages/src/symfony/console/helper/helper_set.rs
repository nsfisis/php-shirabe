use crate::symfony::console::helper::HelperInterface;
use indexmap::IndexMap;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub struct HelperSet {
    helpers: IndexMap<String, Rc<RefCell<dyn HelperInterface>>>,
}

impl HelperSet {
    pub fn new(_helpers: Vec<Rc<RefCell<dyn HelperInterface>>>) -> Self {
        todo!()
    }

    pub fn get<T: HelperInterface + 'static>(&self, _name: &str) -> Rc<RefCell<T>> {
        todo!()
    }

    pub fn set(&mut self, _helper: Rc<RefCell<dyn HelperInterface>>, _alias: Option<&str>) {
        todo!()
    }

    pub fn has(&self, _name: &str) -> bool {
        todo!()
    }
}
