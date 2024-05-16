// Copyright (c) 2024 <Wei Li>.
//
// This source code is licensed under the GNU license found in the
// LICENSE file in the root directory of this source tree.

use core::ops::{Index, IndexMut};

type NodeId = usize;

/// The tree's node type.
pub struct Node<T> {
    pub(crate) parent: Option<NodeId>,
    /// Node id of this node's next sibling node
    pub(crate) next_sibling: Option<NodeId>,
    /// Node id of this node's first child node
    pub(crate) first_child: Option<NodeId>,
    /// Node id of this node's last child node
    pub(crate) last_child: Option<NodeId>,
    /// Associated tree data.
    pub(crate) data: T,
}

impl<T> Node<T> {
    fn new(data: T) -> Self {
        Node {
            parent: None,
            next_sibling: None,
            first_child: None,
            last_child: None,
            data: data,
        }
    }

    /// Returns a reference to the node data.
    pub fn get(&self) -> &T {
        &self.data
    }

    /// Returns a mutable reference to the node data.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }
}

/// A tree structure implemented using a single Vec and numerical identifiers (indices
/// in the vector) instead of reference counted pointers like.
pub struct IndexTree<T> {
    nodes: Vec<Node<T>>,
}

impl<T> IndexTree<T> {
    /// Creates a new empty Tree
    pub fn new_root(data: T) -> IndexTree<T> {
        let root = Node::new(data);
        IndexTree { nodes: vec![root] }
    }

    /// Counts the number of nodes
    #[inline]
    pub fn count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns a reference to the node with the given id if in the tree.
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&Node<T>> {
        self.nodes.get(id)
    }

    /// Returns a mutable reference to the node with the given id if in the tree.
    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node<T>> {
        self.nodes.get_mut(id)
    }

    /// Appends a new child to the node with parent_id, after existing children.
    pub fn add_child(&mut self, parent_id: NodeId, data: T) -> NodeId {
        let child_id = self.new_node(data);
        let parent = self.nodes.get_mut(parent_id).unwrap();
        match parent.last_child {
            None => {
                parent.first_child = Some(child_id);
                parent.last_child = Some(child_id);
            }
            Some(id) => {
                parent.last_child = Some(child_id);
                let last_child = self.nodes.get_mut(id).unwrap();
                last_child.next_sibling = Some(child_id);
            }
        }
        self.nodes.get_mut(child_id).unwrap().parent = Some(parent_id);
        child_id
    }

    /// Returns an iterator of IDs of a given node’s children.
    pub fn children(&self, id: NodeId) -> Children<'_, T> {
        Children::new(self, id)
    }

    /// Finds the child specified by the predicate.
    pub fn find_child<F>(&self, parent_id: NodeId, f: F) -> Option<NodeId>
    where
        F: Fn(&T) -> bool,
    {
        for child in self.children(parent_id) {
            if f(&self[child].data) {
                return Some(child);
            }
        }
        None
    }

    /// An iterator of the IDs of a given node and its descendants, as a pre-order
    /// depth-first search where children are visited in insertion order.
    pub fn descendants(&self, id: NodeId) -> Descendants<'_, T> {
        Descendants::new(self, id)
    }

    fn new_node(&mut self, data: T) -> NodeId {
        let index = self.nodes.len();
        let node = Node::new(data);
        self.nodes.push(node);
        index
    }
}

impl<T> Index<NodeId> for IndexTree<T> {
    type Output = Node<T>;

    fn index(&self, id: NodeId) -> &Node<T> {
        &self.nodes[id]
    }
}

impl<T> IndexMut<NodeId> for IndexTree<T> {
    fn index_mut(&mut self, id: NodeId) -> &mut Node<T> {
        &mut self.nodes[id]
    }
}

macro_rules! impl_node_iterator {
    ($name:ident, $next:expr) => {
        impl<'a, T> Iterator for $name<'a, T> {
            type Item = NodeId;

            fn next(&mut self) -> Option<NodeId> {
                let node = self.node.take()?;
                self.node = $next(&self.tree[node]);
                Some(node)
            }
        }
    };
}

/// An iterator of the IDs of the children of a given node, in insertion order.
pub struct Children<'a, T> {
    tree: &'a IndexTree<T>,
    node: Option<NodeId>,
}

impl<'a, T> Children<'a, T> {
    pub fn new(tree: &'a IndexTree<T>, current: NodeId) -> Self {
        Self {
            tree,
            node: tree[current].first_child,
        }
    }
}

impl_node_iterator!(Children, |node: &Node<T>| node.next_sibling);

/// An iterator of the IDs of a given node and its descendants, as a pre-order depth-first search where children are visited in insertion order.
///
/// i.e. node -> first child -> second child
pub struct Descendants<'a, T>(Traverse<'a, T>, NodeId);

impl<'a, T> Descendants<'a, T> {
    pub(crate) fn new(tree: &'a IndexTree<T>, root: NodeId) -> Self {
        Self(Traverse::new(tree, root), root)
    }
}

impl<'a, T> Iterator for Descendants<'a, T> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        self.0.find_map(|edge| match edge {
            NodeEdge::Start(node) if node == self.1 => None,
            NodeEdge::Start(node) => Some(node),
            NodeEdge::End(_) => None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Indicator if the node is at a start or endpoint of the tree
pub enum NodeEdge {
    /// Indicates that start of a node that has children.
    ///
    /// Yielded by `Traverse::next()` before the node’s descendants.
    Start(NodeId),

    /// Indicates that end of a node that has children.
    ///
    /// Yielded by `Traverse::next()` after the node’s descendants.
    End(NodeId),
}

#[derive(Clone)]
/// An iterator of the "sides" of a node visited during a depth-first pre-order traversal,
/// where node sides are visited start to end and children are visited in insertion order.
///
/// i.e. node.start -> first child -> second child -> node.end
pub struct Traverse<'a, T> {
    tree: &'a IndexTree<T>,
    root: NodeId,
    next: Option<NodeEdge>,
}

impl<'a, T> Traverse<'a, T> {
    pub(crate) fn new(tree: &'a IndexTree<T>, root: NodeId) -> Self {
        Self {
            tree,
            root,
            next: Some(NodeEdge::Start(root)),
        }
    }

    /// Calculates the next node.
    fn next_of_next(&mut self, next: NodeEdge) -> Option<NodeEdge> {
        match next {
            NodeEdge::Start(node) => match self.tree[node].first_child {
                Some(first_child) => Some(NodeEdge::Start(first_child)),
                None => Some(NodeEdge::End(node)),
            },
            NodeEdge::End(node) => {
                if node == self.root {
                    return None;
                }
                let node = &self.tree[node];
                match node.next_sibling {
                    Some(next_sibling) => Some(NodeEdge::Start(next_sibling)),
                    None => node.parent.map(NodeEdge::End),
                }
            }
        }
    }
}

impl<'a, T> Iterator for Traverse<'a, T> {
    type Item = NodeEdge;

    fn next(&mut self) -> Option<NodeEdge> {
        let next = self.next.take()?;
        self.next = self.next_of_next(next);
        Some(next)
    }
}

#[test]
fn index_tree_tests() {
    let mut tree = IndexTree::<u32>::new_root(0);
    tree.add_child(0, 1);
    tree.add_child(0, 2);

    let node = &tree[1];
    assert_eq!(node.data, 1);
    tree.add_child(1, 12);
    tree.add_child(1, 14);
    tree.add_child(1, 15);
    tree.add_child(2, 21);
    tree.add_child(2, 25);
    tree.add_child(2, 27);

    assert_eq!(tree.count(), 9);
    assert_eq!(tree[4].next_sibling, Some(5));
    let node = tree.find_child(1, |&data| data == 14).unwrap();
    assert_eq!(node, 4);
    tree.add_child(node, 143);
    tree.add_child(node, 147);
    let mut children = tree.children(node);
    assert_eq!(tree[children.next().unwrap()].data, 143);
    assert_eq!(tree[children.next().unwrap()].data, 147);
    assert_eq!(children.next(), None);
    let mut descendants = tree.descendants(0);
    // assert_eq!(tree[descendants.next().unwrap()].data, 0);
    assert_eq!(tree[descendants.next().unwrap()].data, 1);
    assert_eq!(tree[descendants.next().unwrap()].data, 12);
    assert_eq!(tree[descendants.next().unwrap()].data, 14);
    assert_eq!(tree[descendants.next().unwrap()].data, 143);
    assert_eq!(tree[descendants.next().unwrap()].data, 147);
    assert_eq!(tree[descendants.next().unwrap()].data, 15);
    assert_eq!(tree[descendants.next().unwrap()].data, 2);
    assert_eq!(tree[descendants.next().unwrap()].data, 21);
    assert_eq!(tree[descendants.next().unwrap()].data, 25);
    assert_eq!(tree[descendants.next().unwrap()].data, 27);
    assert_eq!(descendants.next(), None);
}
