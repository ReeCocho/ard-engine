pub mod textures;

use std::collections::HashSet;

use ard_ecs::prelude::*;
use ard_pal::prelude::*;

type RenderGraphPass = fn(&mut CommandBuffer, &Resources);

pub struct RenderGraph {
    /// All nodes in the graph
    nodes: Vec<RenderGraphNodeInstance>,
    /// Indicies of nodes to run on the next step of the graph.
    to_run: Vec<usize>,
}

pub trait RenderGraphNode {
    fn declare_images(&mut self);

    fn run(&self);
}

#[derive(Default)]
pub struct RenderGraphBuilder {
    /// Name of every node. Used to prevent duplication.
    names: HashSet<String>,
    nodes: Vec<RenderGraphNodeInstance>,
}

struct RenderGraphNodeInstance {
    /// Name of this graph node.
    name: String,
    /// The actual pass function to run.
    pass: RenderGraphPass,
    /// The dependencies this node has.
    deps: Vec<String>,
    /// The number of dependencies that must be run
    deps_remaining: usize,
    /// Indicies within the graph of other nodes that must be notified when this node completes.
    to_notify: Vec<usize>,
}

impl RenderGraph {
    // Run the render graph.
    pub fn run(&mut self, commands: &mut CommandBuffer, resources: &Resources) {
        // Reset the state of the graph
        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.deps_remaining = node.deps.len();
            if node.deps_remaining == 0 {
                self.to_run.push(i);
            }
        }

        // While we still have nodes to run, run them
        while let Some(node_idx) = self.to_run.pop() {
            // Run it
            (self.nodes[node_idx].pass)(commands, resources);

            // Notify every dependency
            // NOTE: We have to do this silly swaping thing with the `to_notify` list because of
            // borrow checker shenanigans. There's probably a better way to do this.
            let to_notify = std::mem::take(&mut self.nodes[node_idx].to_notify);
            for dep_idx in &to_notify {
                let dep = &mut self.nodes[*dep_idx];
                dep.deps_remaining -= 1;

                // If there are no more dependencies for this node, add it to the list
                if dep.deps_remaining == 0 {
                    self.to_run.push(*dep_idx);
                }
            }
            self.nodes[node_idx].to_notify = to_notify;
        }
    }

    fn find_node_idx(&self, name: &str) -> Option<usize> {
        for (i, node) in self.nodes.iter().enumerate() {
            if node.name == name {
                return Some(i);
            }
        }
        None
    }
}

impl RenderGraphBuilder {
    pub fn add_pass(
        mut self,
        name: impl Into<String>,
        pass: RenderGraphPass,
        deps: &[String],
    ) -> Self {
        let name = name.into();

        assert!(
            self.names.insert(name.clone()),
            "render graph node names must be unique"
        );

        self.nodes.push(RenderGraphNodeInstance {
            name,
            pass: pass,
            deps: deps.iter().map(|s| s.clone()).collect(),
            deps_remaining: deps.len(),
            to_notify: Vec::default(),
        });
        self
    }

    pub fn build(self) -> RenderGraph {
        let mut graph = RenderGraph {
            nodes: self.nodes,
            to_run: Vec::default(),
        };

        for i in 0..graph.nodes.len() {
            // Remove dependencies that don't exist
            graph.nodes[i].deps.retain(|name| self.names.contains(name));

            // Find all of `node[i]`s dependencies by index
            let mut deps = Vec::default();
            for name in &graph.nodes[i].deps {
                // Safe to unwrap since we removed non-existant dependencies
                deps.push(graph.find_node_idx(name).unwrap());
            }

            // Update each dependencies `to_notify` list with `node[i]`
            for j in deps {
                graph.nodes[j].to_notify.push(i);
            }
        }

        // Check for circular dependencies. This would imply that all systems have a dependency
        let mut circular = !graph.nodes.is_empty();
        for node in &graph.nodes {
            if node.deps.is_empty() {
                circular = false;
                break;
            }
        }
        assert!(
            !circular,
            "render graph must not have circular dependencies"
        );

        graph
    }
}
