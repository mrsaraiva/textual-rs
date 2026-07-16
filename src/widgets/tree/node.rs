//! Tree identity types: the [`TreeNodeId`] arena key, per-node storage,
//! the [`TreeNode`] declarative seed/builder, the read-only [`NodeRef`] view,
//! and the typed [`TreeError`].

use super::Tree;

slotmap::new_key_type! {
    /// Stable, generational identity of a node inside one [`Tree`] widget.
    ///
    /// Distinct from the widget-tree `NodeId`. Keys are `Copy` and stay valid
    /// across sibling insertion/removal and expansion changes; a removed
    /// node's key reliably misses every lookup (stronger than Python's
    /// reusable `NodeID` int counter). `TreeNodeId::default()` is the null
    /// key: syntactically valid, guaranteed to resolve to nothing.
    pub struct TreeNodeId;
}

/// Arena storage for one tree node (parent link + ordered children).
#[derive(Debug, Clone)]
pub(super) struct TreeNodeData {
    pub(super) label: String,
    pub(super) data: Option<String>,
    pub(super) expanded: bool,
    pub(super) allow_expand: bool,
    pub(super) disabled: bool,
    pub(super) component_classes: Vec<String>,
    /// `None` for a root.
    pub(super) parent: Option<TreeNodeId>,
    /// Ordered children.
    pub(super) children: Vec<TreeNodeId>,
}

impl TreeNodeData {
    pub(super) fn is_expandable(&self) -> bool {
        self.allow_expand || !self.children.is_empty()
    }
}

/// Typed errors for the [`Tree`] identity API.
///
/// Mirrors Python's `UnknownNodeID` / `RemoveRootError` / `AddNodeError`
/// raises as `Result` variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeError {
    /// The id does not resolve to a live node (Python `UnknownNodeID`).
    UnknownNode(TreeNodeId),
    /// Roots cannot be removed (Python `RemoveRootError`).
    RemoveRoot,
    /// A `before`/`after` anchor that is not valid for the operation
    /// (Python `AddNodeError`).
    InvalidAnchor(TreeNodeId),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNode(id) => write!(f, "unknown tree node id {id:?}"),
            Self::RemoveRoot => write!(f, "attempt to remove the tree root"),
            Self::InvalidAnchor(id) => {
                write!(f, "invalid anchor node {id:?} for tree insertion")
            }
        }
    }
}

impl std::error::Error for TreeError {}

/// Read-only view of a live tree node (no back-mutation, so no borrow
/// conflicts). Obtain via [`Tree::node`], [`Tree::get_node_by_id`], or
/// [`Tree::root`]; navigate with [`NodeRef::parent`] / [`NodeRef::children`].
#[derive(Clone, Copy)]
pub struct NodeRef<'a> {
    pub(super) tree: &'a Tree,
    pub(super) id: TreeNodeId,
}

/// Two `NodeRef`s are equal when they view the same node of the same tree
/// instance.
impl PartialEq for NodeRef<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.tree, other.tree) && self.id == other.id
    }
}

impl Eq for NodeRef<'_> {}

impl std::fmt::Debug for NodeRef<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NodeRef")
            .field("id", &self.id)
            .field("label", &self.label())
            .finish()
    }
}

impl<'a> NodeRef<'a> {
    fn node(&self) -> &'a TreeNodeData {
        &self.tree.nodes[self.id]
    }

    /// This node's stable id.
    pub fn id(&self) -> TreeNodeId {
        self.id
    }

    /// The node label.
    pub fn label(&self) -> &'a str {
        &self.node().label
    }

    /// Optional user data.
    pub fn data(&self) -> Option<&'a str> {
        self.node().data.as_deref()
    }

    /// The parent node, or `None` for a root.
    pub fn parent(&self) -> Option<NodeRef<'a>> {
        self.node().parent.map(|id| NodeRef {
            tree: self.tree,
            id,
        })
    }

    /// Iterate this node's children in order.
    pub fn children(&self) -> impl Iterator<Item = NodeRef<'a>> + '_ {
        let tree = self.tree;
        self.node()
            .children
            .iter()
            .map(move |&id| NodeRef { tree, id })
    }

    /// The ordered child ids.
    pub fn child_ids(&self) -> &'a [TreeNodeId] {
        &self.node().children
    }

    /// Number of children.
    pub fn child_count(&self) -> usize {
        self.node().children.len()
    }

    /// Whether this node is a tree root.
    pub fn is_root(&self) -> bool {
        self.node().parent.is_none()
    }

    /// Whether this node is the last of its siblings (Python `is_last`).
    pub fn is_last(&self) -> bool {
        self.tree.is_last(self.id)
    }

    /// Whether this node is currently expanded.
    pub fn is_expanded(&self) -> bool {
        self.node().expanded
    }

    /// Whether the node can be expanded by the user.
    pub fn allow_expand(&self) -> bool {
        self.node().allow_expand
    }

    /// Whether this node is disabled.
    pub fn is_disabled(&self) -> bool {
        self.node().disabled
    }
}

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub(super) label: String,
    pub(super) expanded: bool,
    pub(super) allow_expand: bool,
    pub(super) disabled: bool,
    pub(super) component_classes: Vec<String>,
    pub(super) children: Vec<TreeNode>,
    /// Optional user data associated with this node (e.g. block_id for TOC headings).
    pub(super) data: Option<String>,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            expanded: false,
            allow_expand: false,
            disabled: false,
            component_classes: Vec::new(),
            children: Vec::new(),
            data: None,
        }
    }

    pub fn expanded(mut self, value: bool) -> Self {
        self.expanded = value;
        self
    }

    pub fn with_child(mut self, child: TreeNode) -> Self {
        self.children.push(child);
        self
    }

    pub fn allow_expand(mut self, value: bool) -> Self {
        self.allow_expand = value;
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }

    pub fn with_component_class(mut self, class: impl Into<String>) -> Self {
        self.component_classes.push(class.into());
        self
    }

    /// Set optional user data on this node (builder pattern).
    pub fn with_data(mut self, data: impl Into<String>) -> Self {
        self.data = Some(data.into());
        self
    }

    /// Read-only access to the node's data.
    pub fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    /// Read-only access to this node's children.
    pub fn children_slice(&self) -> &[TreeNode] {
        &self.children
    }

    /// Add a child node, returning a mutable reference to the newly added child.
    ///
    /// This enables the Python pattern of incremental tree construction:
    /// ```ignore
    /// let child = parent.add_child(TreeNode::new("child"));
    /// child.add_child(TreeNode::new("grandchild"));
    /// ```
    pub fn add_child(&mut self, child: TreeNode) -> &mut TreeNode {
        self.children.push(child);
        self.children.last_mut().expect("just pushed")
    }

    /// Add a leaf node (convenience for `add_child(TreeNode::new(label))`).
    pub fn add_leaf(&mut self, label: impl Into<String>) -> &mut TreeNode {
        self.add_child(TreeNode::new(label))
    }

    /// Mutate the node's label after construction.
    ///
    /// Mirrors Python's `node.set_label(text)`.
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Read-only access to the node's label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Expand this node (make children visible).
    ///
    /// Mirrors Python's `node.expand()`.
    pub fn expand(&mut self) {
        self.expanded = true;
    }

    /// Collapse this node (hide children).
    ///
    /// Mirrors Python's `node.collapse()`.
    pub fn collapse(&mut self) {
        self.expanded = false;
    }

    /// Whether this node is currently expanded.
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Set whether this node can be expanded by the user.
    ///
    /// Mirrors Python's `node.allow_expand = value`.
    pub fn set_allow_expand(&mut self, value: bool) {
        self.allow_expand = value;
    }
}
