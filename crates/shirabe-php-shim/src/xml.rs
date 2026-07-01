//! Minimal, PHP-compatible subset of the DOM classes.
//!
//! PHP DOM nodes have reference semantics and form a mutable shared graph, so each node
//! is held behind `Rc<RefCell<_>>`. `DOMDocument`, `DOMNode` and `DOMNodeList` are thin
//! handles over that graph.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

#[derive(Debug, Clone, Copy, PartialEq)]
enum NodeType {
    Document,
    Element,
    Text,
}

#[derive(Debug)]
struct NodeInner {
    node_type: NodeType,
    /// Tag name for elements; unused otherwise.
    name: String,
    /// Ordered attributes for elements (matches setAttribute insertion order).
    attributes: Vec<(String, String)>,
    /// Character data for text nodes.
    value: String,
    children: Vec<Rc<RefCell<NodeInner>>>,
    owner_document: Weak<RefCell<NodeInner>>,
    /// Document-only fields.
    version: String,
    encoding: String,
    format_output: bool,
}

/// A document handle. PHP \DOMDocument.
#[derive(Debug, Clone)]
pub struct DOMDocument(Rc<RefCell<NodeInner>>);

/// A node handle (element or text). PHP \DOMNode or \DOMElement.
#[derive(Debug, Clone)]
pub struct DOMNode(Rc<RefCell<NodeInner>>);

/// An ordered, live-ish snapshot of nodes. PHP \DOMNodeList.
#[derive(Debug, Clone)]
pub struct DOMNodeList(Vec<DOMNode>);

impl DOMDocument {
    pub fn new(version: &str, encoding: &str) -> DOMDocument {
        DOMDocument(Rc::new(RefCell::new(NodeInner {
            node_type: NodeType::Document,
            name: String::new(),
            attributes: Vec::new(),
            value: String::new(),
            children: Vec::new(),
            owner_document: Weak::new(),
            version: version.to_string(),
            encoding: encoding.to_string(),
            format_output: false,
        })))
    }

    /// View the document as a \DOMNode.
    pub fn as_node(&self) -> DOMNode {
        DOMNode(self.0.clone())
    }

    pub fn create_element(&self, name: &str) -> DOMNode {
        DOMNode(Rc::new(RefCell::new(NodeInner {
            node_type: NodeType::Element,
            name: name.to_string(),
            attributes: Vec::new(),
            value: String::new(),
            children: Vec::new(),
            owner_document: Rc::downgrade(&self.0),
            version: String::new(),
            encoding: String::new(),
            format_output: false,
        })))
    }

    pub fn create_element_with_value(&self, name: &str, value: &str) -> DOMNode {
        let element = self.create_element(name);
        let text = self.create_text_node(value);
        element.0.borrow_mut().children.push(text.0);
        element
    }

    pub fn create_text_node(&self, data: &str) -> DOMNode {
        DOMNode(Rc::new(RefCell::new(NodeInner {
            node_type: NodeType::Text,
            name: String::new(),
            attributes: Vec::new(),
            value: data.to_string(),
            children: Vec::new(),
            owner_document: Rc::downgrade(&self.0),
            version: String::new(),
            encoding: String::new(),
            format_output: false,
        })))
    }

    pub fn append_child(&self, child: DOMNode) -> DOMNode {
        self.0.borrow_mut().children.push(child.0.clone());
        child
    }

    /// Deep- or shallow-copy a node so it can be inserted into this document.
    pub fn import_node(&self, node: &DOMNode, deep: bool) -> DOMNode {
        DOMNode(deep_clone(&node.0, &Rc::downgrade(&self.0), deep))
    }

    pub fn get_elements_by_tag_name(&self, name: &str) -> DOMNodeList {
        let mut out = Vec::new();
        collect_by_tag(&self.0, name, &mut out);
        DOMNodeList(out.into_iter().map(DOMNode).collect())
    }

    /// Corresponds to the PHP `$dom->formatOutput` property.
    pub fn set_format_output(&self, value: bool) {
        self.0.borrow_mut().format_output = value;
    }

    pub fn save_xml<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let doc = self.0.borrow();
        writeln!(
            writer,
            "<?xml version=\"{}\" encoding=\"{}\"?>",
            doc.version, doc.encoding
        )?;
        for child in &doc.children {
            serialize_node(child, 0, doc.format_output, writer)?;
            writer.write_all(b"\n")?;
        }
        Ok(())
    }
}

impl DOMNode {
    pub fn append_child(&self, child: DOMNode) -> DOMNode {
        self.0.borrow_mut().children.push(child.0.clone());
        child
    }

    pub fn set_attribute(&self, name: &str, value: &str) {
        let mut node = self.0.borrow_mut();
        if let Some(attr) = node.attributes.iter_mut().find(|(k, _)| k == name) {
            attr.1 = value.to_string();
        } else {
            node.attributes.push((name.to_string(), value.to_string()));
        }
    }

    pub fn child_nodes(&self) -> DOMNodeList {
        DOMNodeList(
            self.0
                .borrow()
                .children
                .iter()
                .map(|c| DOMNode(c.clone()))
                .collect(),
        )
    }

    pub fn owner_document(&self) -> DOMDocument {
        DOMDocument(
            self.0
                .borrow()
                .owner_document
                .upgrade()
                .expect("owner document has been dropped"),
        )
    }

    pub fn get_elements_by_tag_name(&self, name: &str) -> DOMNodeList {
        let mut out = Vec::new();
        collect_by_tag(&self.0, name, &mut out);
        DOMNodeList(out.into_iter().map(DOMNode).collect())
    }
}

impl DOMNodeList {
    pub fn item(&self, index: usize) -> Option<DOMNode> {
        self.0.get(index).cloned()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, DOMNode> {
        self.0.iter()
    }
}

impl IntoIterator for DOMNodeList {
    type Item = DOMNode;
    type IntoIter = std::vec::IntoIter<DOMNode>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

fn deep_clone(
    node: &Rc<RefCell<NodeInner>>,
    owner: &Weak<RefCell<NodeInner>>,
    deep: bool,
) -> Rc<RefCell<NodeInner>> {
    let source = node.borrow();
    let children = if deep {
        source
            .children
            .iter()
            .map(|c| deep_clone(c, owner, true))
            .collect()
    } else {
        Vec::new()
    };
    Rc::new(RefCell::new(NodeInner {
        node_type: source.node_type,
        name: source.name.clone(),
        attributes: source.attributes.clone(),
        value: source.value.clone(),
        children,
        owner_document: owner.clone(),
        version: source.version.clone(),
        encoding: source.encoding.clone(),
        format_output: source.format_output,
    }))
}

fn collect_by_tag(
    node: &Rc<RefCell<NodeInner>>,
    name: &str,
    out: &mut Vec<Rc<RefCell<NodeInner>>>,
) {
    for child in &node.borrow().children {
        let is_match = {
            let c = child.borrow();
            c.node_type == NodeType::Element && (name == "*" || c.name == name)
        };
        if is_match {
            out.push(child.clone());
        }
        collect_by_tag(child, name, out);
    }
}

fn serialize_node<W: std::io::Write>(
    node: &Rc<RefCell<NodeInner>>,
    depth: usize,
    format: bool,
    out: &mut W,
) -> std::io::Result<()> {
    let node = node.borrow();
    match node.node_type {
        NodeType::Text => escape_text(&node.value, out)?,
        NodeType::Element => {
            out.write_all(b"<")?;
            out.write_all(node.name.as_bytes())?;
            for (key, value) in &node.attributes {
                out.write_all(b" ")?;
                out.write_all(key.as_bytes())?;
                out.write_all(b"=\"")?;
                escape_attribute(value, out)?;
                out.write_all(b"\"")?;
            }
            if node.children.is_empty() {
                out.write_all(b"/>")?;
                return Ok(());
            }
            out.write_all(b">")?;
            // libxml only pretty-prints an element whose children are all non-text.
            let has_text_child = node
                .children
                .iter()
                .any(|c| c.borrow().node_type == NodeType::Text);
            if format && !has_text_child {
                for child in &node.children {
                    out.write_all(b"\n")?;
                    write_indent(out, depth + 1)?;
                    serialize_node(child, depth + 1, format, out)?;
                }
                out.write_all(b"\n")?;
                write_indent(out, depth)?;
            } else {
                for child in &node.children {
                    serialize_node(child, depth, format, out)?;
                }
            }
            out.write_all(b"</")?;
            out.write_all(node.name.as_bytes())?;
            out.write_all(b">")?;
        }
        NodeType::Document => {
            for child in &node.children {
                serialize_node(child, depth, format, out)?;
            }
        }
    }
    Ok(())
}

fn write_indent<W: std::io::Write>(out: &mut W, depth: usize) -> std::io::Result<()> {
    for _ in 0..depth {
        out.write_all(b"  ")?;
    }
    Ok(())
}

fn escape_text<W: std::io::Write>(s: &str, out: &mut W) -> std::io::Result<()> {
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '&' => out.write_all(b"&amp;")?,
            '<' => out.write_all(b"&lt;")?,
            '>' => out.write_all(b"&gt;")?,
            '\r' => out.write_all(b"&#13;")?,
            _ => out.write_all(c.encode_utf8(&mut buf).as_bytes())?,
        }
    }
    Ok(())
}

fn escape_attribute<W: std::io::Write>(s: &str, out: &mut W) -> std::io::Result<()> {
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            '&' => out.write_all(b"&amp;")?,
            '<' => out.write_all(b"&lt;")?,
            '>' => out.write_all(b"&gt;")?,
            '"' => out.write_all(b"&quot;")?,
            '\r' => out.write_all(b"&#13;")?,
            '\n' => out.write_all(b"&#10;")?,
            '\t' => out.write_all(b"&#9;")?,
            _ => out.write_all(c.encode_utf8(&mut buf).as_bytes())?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_output_matches_libxml() {
        let dom = DOMDocument::new("1.0", "UTF-8");
        let definition = dom.append_child(dom.create_element("definition"));

        let arguments = definition.append_child(dom.create_element("arguments"));
        let argument = arguments.append_child(dom.create_element("argument"));
        argument.set_attribute("name", "foo");
        argument.set_attribute("is_required", "1");
        argument.set_attribute("is_array", "0");
        let description = argument.append_child(dom.create_element("description"));
        description.append_child(dom.create_text_node("The foo arg"));
        argument.append_child(dom.create_element("defaults"));

        definition.append_child(dom.create_element("options"));

        dom.set_format_output(true);

        let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<definition>\n\
\x20\x20<arguments>\n\
\x20\x20\x20\x20<argument name=\"foo\" is_required=\"1\" is_array=\"0\">\n\
\x20\x20\x20\x20\x20\x20<description>The foo arg</description>\n\
\x20\x20\x20\x20\x20\x20<defaults/>\n\
\x20\x20\x20\x20</argument>\n\
\x20\x20</arguments>\n\
\x20\x20<options/>\n\
</definition>\n";
        let mut out = Vec::new();
        dom.save_xml(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn text_and_create_element_value_are_escaped() {
        let dom = DOMDocument::new("1.0", "UTF-8");
        let root = dom.append_child(dom.create_element("root"));
        let escaped = root.append_child(dom.create_element("escaped"));
        escaped.append_child(dom.create_text_node("a < b & c > d"));
        // <usage> elements carry synopsis strings such as "<package>"; libxml escapes the
        // angle brackets of the createElement value too.
        root.append_child(dom.create_element_with_value("usage", "cmd <package>"));
        dom.set_format_output(true);

        let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<root>\n\
\x20\x20<escaped>a &lt; b &amp; c &gt; d</escaped>\n\
\x20\x20<usage>cmd &lt;package&gt;</usage>\n\
</root>\n";
        let mut out = Vec::new();
        dom.save_xml(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }

    #[test]
    fn import_node_deep_copies_and_get_elements_by_tag_name() {
        let source = DOMDocument::new("1.0", "UTF-8");
        let wrapper = source.append_child(source.create_element("definition"));
        let inner = wrapper.append_child(source.create_element("inner"));
        inner.set_attribute("k", "v");

        let target = DOMDocument::new("1.0", "UTF-8");
        let host = target.append_child(target.create_element("command"));

        // Emulate XmlDescriptor::appendDocument over the <definition> subtree.
        let found = source
            .get_elements_by_tag_name("definition")
            .item(0)
            .unwrap();
        for child in found.child_nodes() {
            host.append_child(host.owner_document().import_node(&child, true));
        }

        // Mutating the source after import must not affect the imported copy.
        inner.set_attribute("k", "changed");
        target.set_format_output(true);

        let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<command>\n\
\x20\x20<inner k=\"v\"/>\n\
</command>\n";
        let mut out = Vec::new();
        target.save_xml(&mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), expected);
    }
}
