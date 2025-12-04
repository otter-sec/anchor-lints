use anchor_lints_utils::{mir_analyzer::MirAnalyzer, models::AccountNameAndLocal};
use rustc_middle::mir::{BasicBlock, BasicBlocks, Local};

use std::collections::{HashMap, HashSet, VecDeque};

// Checks if a block is reachable from another block.
pub fn reachable_block(graph: &BasicBlocks, from: BasicBlock, to: BasicBlock) -> bool {
    is_reachable_from(graph, from, |bb| bb == to)
}

// Checks if a HashSet of blocks is reachable from another block.
pub fn reachable_blocks(graph: &BasicBlocks, from: BasicBlock, to: &HashSet<BasicBlock>) -> bool {
    is_reachable_from(graph, from, |bb| to.contains(&bb))
}

// Combine reachable_block and reachable_blocks into a single generic function:
pub fn is_reachable_from(
    graph: &BasicBlocks,
    from: BasicBlock,
    target: impl Fn(BasicBlock) -> bool,
) -> bool {
    let mut queue = VecDeque::from([from]);
    let mut visited = HashSet::from([from]);

    while let Some(u) = queue.pop_front() {
        if target(u) {
            return true;
        }
        if let Some(terminator) = &graph[u].terminator {
            for succ in terminator.successors() {
                if visited.insert(succ) {
                    queue.push_back(succ);
                }
            }
        }
    }
    false
}

/// Finds blocks in `to` that are reachable from `from` nodes without passing through `without` nodes
/// Returns a list of `to` nodes with the `from` node they are reachable from
pub fn reachable_without_passing(
    graph: &BasicBlocks,
    from: HashSet<BasicBlock>,
    to: HashSet<BasicBlock>,
    without: HashSet<BasicBlock>,
) -> Vec<(BasicBlock, BasicBlock)> {
    let mut queue = VecDeque::new();
    // Map of nodes to the `from` block they are reachable from
    let mut origin = HashMap::new();
    let mut visited = HashSet::new();

    for &f in &from {
        origin.insert(f, f);
        visited.insert(f);
        queue.push_back(f);
    }

    while let Some(u) = queue.pop_front() {
        if without.contains(&u) {
            continue;
        }
        if let Some(terminator) = &graph[u].terminator {
            for succ in terminator.successors() {
                if without.contains(&succ) || visited.contains(&succ) {
                    continue;
                }
                origin.insert(succ, origin[&u]);
                visited.insert(succ);
                queue.push_back(succ);
            }
        }
    }

    to.into_iter()
        .filter_map(|bb| origin.get(&bb).map(|o| (bb, *o)))
        .collect()
}

pub fn extract_account_name_from_local<'tcx>(
    mir_analyzer: &MirAnalyzer<'_, 'tcx>,
    local: &Local,
    return_only_name: bool,
) -> Option<AccountNameAndLocal> {
    let account_name_and_locals = mir_analyzer.check_local_and_assignment_locals(
        local,
        &mut HashSet::new(),
        return_only_name,
        &mut String::new(),
    );
    account_name_and_locals.first().cloned()
}
