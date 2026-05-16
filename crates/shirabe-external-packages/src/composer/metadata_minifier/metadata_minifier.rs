use indexmap::IndexMap;
use shirabe_php_shim::PhpMixed;

#[derive(Debug)]
pub struct MetadataMinifier;

impl MetadataMinifier {
    pub fn expand(minified_data: IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed> {
        todo!()
    }

    pub fn minify(packages: IndexMap<String, PhpMixed>) -> IndexMap<String, PhpMixed> {
        todo!()
    }
}
