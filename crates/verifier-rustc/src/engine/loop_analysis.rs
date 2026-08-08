use std::collections::HashMap;

use rustc_middle::mir::{BasicBlock, Body, Local, START_BLOCK, StatementKind};

use crate::contracts::{Clause, FunctionSpec};

#[derive(Debug, Clone)]
pub(crate) struct LoopInfo {
    pub header: BasicBlock,
    pub blocks: Vec<bool>,
    pub modified_locals: Vec<Local>,
    pub invariants: Vec<Clause>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LoopAnalysis {
    loops: Vec<LoopInfo>,
    by_header: HashMap<BasicBlock, usize>,
    backedges: HashMap<(BasicBlock, BasicBlock), usize>,
}

impl LoopAnalysis {
    pub fn new(body: &Body<'_>, spec: &FunctionSpec) -> Result<Self, String> {
        let block_count = body.basic_blocks.len();
        let mut predecessors = vec![Vec::new(); block_count];
        for (source, data) in body.basic_blocks.iter_enumerated() {
            for target in data.terminator().successors() {
                predecessors[target.index()].push(source);
            }
        }

        let dominators = compute_dominators(body, &predecessors);
        let mut latches_by_header: HashMap<BasicBlock, Vec<BasicBlock>> = HashMap::new();
        for (source, data) in body.basic_blocks.iter_enumerated() {
            for target in data.terminator().successors() {
                if dominators[source.index()][target.index()] {
                    latches_by_header.entry(target).or_default().push(source);
                }
            }
        }

        let mut loops = Vec::new();
        for (header, latches) in latches_by_header {
            let blocks = natural_loop_blocks(header, &latches, &predecessors, block_count);
            let modified_locals = modified_locals(body, &blocks);
            loops.push(LoopInfo { header, blocks, modified_locals, invariants: Vec::new() });
        }
        loops.sort_by_key(|info| info.header.index());

        for loop_spec in &spec.loops {
            let mut candidates = loops
                .iter()
                .enumerate()
                .filter_map(|(index, info)| {
                    let header_span = body.basic_blocks[info.header].terminator().source_info.span;
                    loop_spec.span.contains(header_span).then_some((index, header_span.lo()))
                })
                .collect::<Vec<_>>();
            candidates.sort_by_key(|(_, lo)| *lo);
            let Some((index, _)) = candidates.first().copied() else {
                return Err("could not map loop invariant to a MIR loop header".to_owned());
            };
            if !loops[index].invariants.is_empty() {
                return Err(
                    "multiple annotated source loops map to the same MIR loop header".to_owned()
                );
            }
            loops[index].invariants = loop_spec.invariants.clone();
        }

        let mut analysis = Self { loops, ..Self::default() };
        for (index, info) in analysis.loops.iter().enumerate() {
            analysis.by_header.insert(info.header, index);
            for source in body.basic_blocks.indices() {
                if info.blocks[source.index()]
                    && body.basic_blocks[source]
                        .terminator()
                        .successors()
                        .any(|target| target == info.header)
                {
                    analysis.backedges.insert((source, info.header), index);
                }
            }
        }
        Ok(analysis)
    }

    pub fn header(&self, block: BasicBlock) -> Option<&LoopInfo> {
        self.by_header.get(&block).map(|index| &self.loops[*index])
    }

    pub fn backedge(&self, source: BasicBlock, target: BasicBlock) -> Option<&LoopInfo> {
        self.backedges.get(&(source, target)).map(|index| &self.loops[*index])
    }

    pub fn is_external_entry(&self, source: BasicBlock, info: &LoopInfo) -> bool {
        !info.blocks[source.index()] || source == info.header && source == START_BLOCK
    }
}

fn compute_dominators(body: &Body<'_>, predecessors: &[Vec<BasicBlock>]) -> Vec<Vec<bool>> {
    let count = body.basic_blocks.len();
    let mut dominators = vec![vec![true; count]; count];
    dominators[START_BLOCK.index()].fill(false);
    dominators[START_BLOCK.index()][START_BLOCK.index()] = true;

    let mut changed = true;
    while changed {
        changed = false;
        for block in body.basic_blocks.indices() {
            if block == START_BLOCK {
                continue;
            }
            let preds = &predecessors[block.index()];
            let mut next = vec![true; count];
            if preds.is_empty() {
                next.fill(false);
            } else {
                for pred in preds {
                    for index in 0..count {
                        next[index] &= dominators[pred.index()][index];
                    }
                }
            }
            next[block.index()] = true;
            if next != dominators[block.index()] {
                dominators[block.index()] = next;
                changed = true;
            }
        }
    }
    dominators
}

fn natural_loop_blocks(
    header: BasicBlock,
    latches: &[BasicBlock],
    predecessors: &[Vec<BasicBlock>],
    count: usize,
) -> Vec<bool> {
    let mut blocks = vec![false; count];
    blocks[header.index()] = true;
    let mut pending = Vec::new();
    for latch in latches {
        if !blocks[latch.index()] {
            blocks[latch.index()] = true;
            pending.push(*latch);
        }
    }
    while let Some(block) = pending.pop() {
        for predecessor in &predecessors[block.index()] {
            if !blocks[predecessor.index()] {
                blocks[predecessor.index()] = true;
                pending.push(*predecessor);
            }
        }
    }
    blocks
}

fn modified_locals(body: &Body<'_>, blocks: &[bool]) -> Vec<Local> {
    let mut modified = vec![false; body.local_decls.len()];
    for block in body.basic_blocks.indices().filter(|block| blocks[block.index()]) {
        for statement in &body.basic_blocks[block].statements {
            match &statement.kind {
                StatementKind::Assign(assignment) => modified[assignment.0.local.index()] = true,
                StatementKind::SetDiscriminant { place, .. } => {
                    modified[place.local.index()] = true
                }
                _ => {}
            }
        }
    }
    body.local_decls.indices().filter(|local| modified[local.index()]).collect()
}

#[cfg(test)]
mod tests {
    use super::natural_loop_blocks;
    use rustc_middle::mir::BasicBlock;

    #[test]
    fn collects_a_natural_loop_from_its_latch() {
        let header = BasicBlock::from_usize(1);
        let latch = BasicBlock::from_usize(3);
        let predecessors = vec![
            vec![],
            vec![BasicBlock::from_usize(0), latch],
            vec![header],
            vec![BasicBlock::from_usize(2)],
        ];
        let blocks = natural_loop_blocks(header, &[latch], &predecessors, 4);
        assert_eq!(blocks, vec![false, true, true, true]);
    }
}
