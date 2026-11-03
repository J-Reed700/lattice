//! A class's graph: nodes of one step each, and where each outcome leads.

use super::{Next, NodeId, Outcome, Step};
use crate::domain::learning::claim_review::Class;

/// One step of a class's check, what it costs in model calls, and where each
/// of its outcomes leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: &'static str,
    pub step: Step,
    /// Model calls the step costs.
    pub calls: u32,
    edges: Vec<(Outcome, Next)>,
}

impl Node {
    pub fn new(name: &'static str, step: Step, calls: u32) -> Self {
        Self {
            name,
            step,
            calls,
            edges: Vec::new(),
        }
    }

    pub fn id(&self) -> NodeId {
        NodeId(self.name)
    }

    /// Sends each of `outcomes` to `next`, replacing any edge it had.
    pub fn on(mut self, outcomes: &[Outcome], next: Next) -> Self {
        for &outcome in outcomes {
            self.edges.retain(|edge| edge.0 != outcome);
            self.edges.push((outcome, next));
        }
        self
    }

    /// Sends every outcome the step can give that has no edge yet to `next`.
    pub fn otherwise(mut self, next: Next) -> Self {
        for &outcome in self.step.outcomes() {
            if !self.edges.iter().any(|edge| edge.0 == outcome) {
                self.edges.push((outcome, next));
            }
        }
        self
    }

    /// Where `outcome` leads, when the node has an edge for it.
    pub fn next(&self, outcome: Outcome) -> Option<Next> {
        self.edges
            .iter()
            .find(|edge| edge.0 == outcome)
            .map(|edge| edge.1)
    }

    pub fn edges(&self) -> &[(Outcome, Next)] {
        &self.edges
    }
}

/// The check of one class: its entry, its nodes, and the node a rewrite's
/// successor continues at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    class: Class,
    entry: NodeId,
    after_rewrite: NodeId,
    nodes: Vec<Node>,
}

impl Graph {
    pub fn new(class: Class, entry: &'static str, after_rewrite: &'static str) -> Self {
        Self {
            class,
            entry: NodeId(entry),
            after_rewrite: NodeId(after_rewrite),
            nodes: Vec::new(),
        }
    }

    /// Adds `node`, replacing any node of its name.
    pub fn with(mut self, node: Node) -> Self {
        self.nodes.retain(|have| have.name != node.name);
        self.nodes.push(node);
        self
    }

    pub fn class(&self) -> Class {
        self.class
    }

    pub fn entry(&self) -> NodeId {
        self.entry
    }

    /// Where a rewrite's successor, and a statement whose sentence a
    /// neighbour's rewrite changed, continues.
    pub fn after_rewrite(&self) -> NodeId {
        self.after_rewrite
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.name == id.0)
    }

    /// The id of the node named `name`: how a name read back from a record
    /// becomes a node of this graph.
    pub fn id_of(&self, name: &str) -> Option<NodeId> {
        self.nodes
            .iter()
            .find(|node| node.name == name)
            .map(Node::id)
    }
}
