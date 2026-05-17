use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct MetadataMinifier;

impl MetadataMinifier {
    pub fn expand(_minified_data: IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    pub fn minify(_packages: IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed> {
        todo!()
    }
}
