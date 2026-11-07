//! Minimal, PHP-compatible subset of the DOM classes.

use crate::bconcat;
use bstr::{BStr, BString};
use std::rc::Weak;

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
    name: BString,
    /// Ordered attributes for elements (matches setAttribute insertion order).
    attributes: Vec<(BString, BString)>,
    /// Character data for text nodes.
    value: BString,
    children: Vec<std::rc::Rc<std::cell::RefCell<NodeInner>>>,
    owner_document: Weak<std::cell::RefCell<NodeInner>>,
    /// Document-only fields.
    version: BString,
    encoding: BString,
    format_output: bool,
}

/// A document handle. PHP \DOMDocument.
#[derive(Debug, Clone)]
pub struct DOMDocument(std::rc::Rc<std::cell::RefCell<NodeInner>>);

/// A node handle (element or text). PHP \DOMNode or \DOMElement.
#[derive(Debug, Clone)]
pub struct DOMNode(std::rc::Rc<std::cell::RefCell<NodeInner>>);

/// An ordered, live-ish snapshot of nodes. PHP \DOMNodeList.
#[derive(Debug, Clone)]
pub struct DOMNodeList(Vec<DOMNode>);

impl DOMDocument {
    pub fn new(version: impl AsRef<[u8]>, encoding: impl AsRef<[u8]>) -> DOMDocument {
        let version = BStr::new(version.as_ref());
        let encoding = BStr::new(encoding.as_ref());
        DOMDocument(std::rc::Rc::new(std::cell::RefCell::new(NodeInner {
            node_type: NodeType::Document,
            name: BString::default(),
            attributes: Vec::new(),
            value: BString::default(),
            children: Vec::new(),
            owner_document: Weak::new(),
            version: BString::from(version),
            encoding: BString::from(encoding),
            format_output: false,
        })))
    }

    /// View the document as a \DOMNode.
    pub fn as_node(&self) -> DOMNode {
        DOMNode(self.0.clone())
    }

    pub fn create_element(&self, name: impl AsRef<[u8]>) -> DOMNode {
        let name = BStr::new(name.as_ref());
        DOMNode(std::rc::Rc::new(std::cell::RefCell::new(NodeInner {
            node_type: NodeType::Element,
            name: BString::from(name),
            attributes: Vec::new(),
            value: BString::default(),
            children: Vec::new(),
            owner_document: std::rc::Rc::downgrade(&self.0),
            version: BString::default(),
            encoding: BString::default(),
            format_output: false,
        })))
    }

    pub fn create_element_with_value(
        &self,
        name: impl AsRef<[u8]>,
        value: impl AsRef<[u8]>,
    ) -> DOMNode {
        let name = BStr::new(name.as_ref());
        let element = self.create_element(name);
        let text = self.create_text_node(value);
        element.0.borrow_mut().children.push(text.0);
        element
    }

    pub fn create_text_node(&self, data: impl AsRef<[u8]>) -> DOMNode {
        DOMNode(std::rc::Rc::new(std::cell::RefCell::new(NodeInner {
            node_type: NodeType::Text,
            name: BString::default(),
            attributes: Vec::new(),
            value: BString::from(data.as_ref()),
            children: Vec::new(),
            owner_document: std::rc::Rc::downgrade(&self.0),
            version: BString::default(),
            encoding: BString::default(),
            format_output: false,
        })))
    }

    pub fn append_child(&self, child: DOMNode) -> DOMNode {
        self.0.borrow_mut().children.push(child.0.clone());
        child
    }

    /// Deep- or shallow-copy a node so it can be inserted into this document.
    pub fn import_node(&self, node: &DOMNode, deep: bool) -> DOMNode {
        DOMNode(deep_clone(&node.0, &std::rc::Rc::downgrade(&self.0), deep))
    }

    pub fn get_elements_by_tag_name(&self, name: impl AsRef<[u8]>) -> DOMNodeList {
        let name = BStr::new(name.as_ref());
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
        writer.write_all(&bconcat!(
            "<?xml version=\"",
            doc.version,
            "\" encoding=\"",
            doc.encoding,
            "\"?>\n"
        ))?;
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

    pub fn set_attribute(&self, name: impl AsRef<[u8]>, value: impl AsRef<[u8]>) {
        let name = BStr::new(name.as_ref());
        let value = BString::from(value.as_ref());
        let mut node = self.0.borrow_mut();
        if let Some(attr) = node.attributes.iter_mut().find(|(k, _)| k == name) {
            attr.1 = value;
        } else {
            node.attributes.push((BString::from(name), value));
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

    pub fn get_elements_by_tag_name(&self, name: impl AsRef<[u8]>) -> DOMNodeList {
        let name = BStr::new(name.as_ref());
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
    node: &std::rc::Rc<std::cell::RefCell<NodeInner>>,
    owner: &Weak<std::cell::RefCell<NodeInner>>,
    deep: bool,
) -> std::rc::Rc<std::cell::RefCell<NodeInner>> {
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
    std::rc::Rc::new(std::cell::RefCell::new(NodeInner {
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
    node: &std::rc::Rc<std::cell::RefCell<NodeInner>>,
    name: &BStr,
    out: &mut Vec<std::rc::Rc<std::cell::RefCell<NodeInner>>>,
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
    node: &std::rc::Rc<std::cell::RefCell<NodeInner>>,
    depth: usize,
    format: bool,
    out: &mut W,
) -> std::io::Result<()> {
    let node = node.borrow();
    match node.node_type {
        NodeType::Text => escape_text(&node.value, out)?,
        NodeType::Element => {
            out.write_all(b"<")?;
            out.write_all(&node.name)?;
            for (key, value) in &node.attributes {
                out.write_all(b" ")?;
                out.write_all(key)?;
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
            out.write_all(&node.name)?;
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

fn escape_text<W: std::io::Write>(s: &[u8], out: &mut W) -> std::io::Result<()> {
    for &b in s {
        match b {
            b'&' => out.write_all(b"&amp;")?,
            b'<' => out.write_all(b"&lt;")?,
            b'>' => out.write_all(b"&gt;")?,
            b'\r' => out.write_all(b"&#13;")?,
            _ => out.write_all(&[b])?,
        }
    }
    Ok(())
}

fn escape_attribute<W: std::io::Write>(s: &[u8], out: &mut W) -> std::io::Result<()> {
    for &b in s {
        match b {
            b'&' => out.write_all(b"&amp;")?,
            b'<' => out.write_all(b"&lt;")?,
            b'>' => out.write_all(b"&gt;")?,
            b'"' => out.write_all(b"&quot;")?,
            b'\r' => out.write_all(b"&#13;")?,
            b'\n' => out.write_all(b"&#10;")?,
            b'\t' => out.write_all(b"&#9;")?,
            _ => out.write_all(&[b])?,
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
        assert_eq!(bstr::BStr::new(&out), bstr::BStr::new(expected));
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
        assert_eq!(bstr::BStr::new(&out), bstr::BStr::new(expected));
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
        assert_eq!(bstr::BStr::new(&out), bstr::BStr::new(expected));
    }
}
