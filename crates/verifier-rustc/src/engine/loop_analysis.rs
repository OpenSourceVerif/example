use std::fmt;

use rustc_index::{IndexVec, bit_set::DenseBitSet};
use rustc_middle::mir::{BasicBlock, Body, Local, START_BLOCK, StatementKind};
use smallvec::SmallVec;

use crate::spec::{Clause, Spec};

#[derive(Debug, Clone)]
pub(crate) struct LoopInfo {
    pub header: BasicBlock,
    pub blocks: DenseBitSet<BasicBlock>,
    pub modified: DenseBitSet<Local>,
    pub invariants: Vec<Clause>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopError {
    Count { spec: usize, mir: usize },
}

impl fmt::Display for LoopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Count { spec, mir } => {
                write!(f, "found {spec} source loops but {mir} MIR loops")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoopAnalysis {
    loops: Vec<LoopInfo>,
    by_header: IndexVec<BasicBlock, Option<usize>>,
    backedges: IndexVec<BasicBlock, SmallVec<[(BasicBlock, usize); 2]>>,
}

impl LoopAnalysis {
    pub fn new(body: &Body<'_>, spec: &Spec) -> Result<Self, LoopError> {
        let count = body.basic_blocks.len();
        let dominators = body.basic_blocks.dominators();
        let predecessors = body.basic_blocks.predecessors();
        let mut latches = IndexVec::from_elem_n(SmallVec::<[BasicBlock; 2]>::new(), count);

        for (source, data) in body.basic_blocks.iter_enumerated() {
            let reachable =
                source == START_BLOCK || dominators.immediate_dominator(source).is_some();
            if !reachable {
                continue;
            }
            for target in data.terminator().successors() {
                if dominators.dominates(target, source) {
                    latches[target].push(source);
                }
            }
        }

        let mut loops = latches
            .iter_enumerated()
            .filter(|(_, latches)| !latches.is_empty())
            .map(|(header, latches)| {
                let blocks = natural_loop(header, latches, |block| &predecessors[block], count);
                let modified = modified(body, &blocks);
                LoopInfo { header, blocks, modified, invariants: Vec::new() }
            })
            .collect::<Vec<_>>();
        loops.sort_by_key(|info| body.basic_blocks[info.header].terminator().source_info.span.lo());

        if spec.loops.len() != loops.len() {
            return Err(LoopError::Count { spec: spec.loops.len(), mir: loops.len() });
        }
        for (info, loop_spec) in loops.iter_mut().zip(&spec.loops) {
            info.invariants.clone_from(&loop_spec.invariants);
        }

        let mut by_header = IndexVec::from_elem_n(None, count);
        let mut backedges = IndexVec::from_elem_n(SmallVec::new(), count);
        for (index, info) in loops.iter().enumerate() {
            by_header[info.header] = Some(index);
            for source in info.blocks.iter() {
                if body.basic_blocks[source]
                    .terminator()
                    .successors()
                    .any(|target| target == info.header)
                {
                    backedges[source].push((info.header, index));
                }
            }
        }

        Ok(Self { loops, by_header, backedges })
    }

    pub fn header(&self, block: BasicBlock) -> Option<&LoopInfo> {
        self.by_header[block].map(|index| &self.loops[index])
    }

    pub fn backedge(&self, source: BasicBlock, target: BasicBlock) -> Option<&LoopInfo> {
        self.backedges[source]
            .iter()
            .find_map(|(header, index)| (*header == target).then(|| &self.loops[*index]))
    }

    pub fn is_external_entry(&self, source: BasicBlock, info: &LoopInfo) -> bool {
        !info.blocks.contains(source) || source == info.header && source == START_BLOCK
    }
}

fn natural_loop<'a>(
    header: BasicBlock,
    latches: &[BasicBlock],
    predecessors: impl Fn(BasicBlock) -> &'a [BasicBlock],
    count: usize,
) -> DenseBitSet<BasicBlock> {
    let mut blocks = DenseBitSet::new_empty(count);
    blocks.insert(header);
    let mut pending = SmallVec::<[BasicBlock; 8]>::new();
    for &latch in latches {
        if blocks.insert(latch) {
            pending.push(latch);
        }
    }
    while let Some(block) = pending.pop() {
        for &predecessor in predecessors(block) {
            if blocks.insert(predecessor) {
                pending.push(predecessor);
            }
        }
    }
    blocks
}

fn modified(body: &Body<'_>, blocks: &DenseBitSet<BasicBlock>) -> DenseBitSet<Local> {
    let mut modified = DenseBitSet::new_empty(body.local_decls.len());
    for block in blocks.iter() {
        for statement in &body.basic_blocks[block].statements {
            match &statement.kind {
                StatementKind::Assign(assignment) => {
                    modified.insert(assignment.0.local);
                }
                StatementKind::SetDiscriminant { place, .. } => {
                    modified.insert(place.local);
                }
                StatementKind::FakeRead(_)
                | StatementKind::Intrinsic(_)
                | StatementKind::StorageLive(_)
                | StatementKind::StorageDead(_)
                | StatementKind::PlaceMention(_)
                | StatementKind::AscribeUserType(_, _)
                | StatementKind::Coverage(_)
                | StatementKind::ConstEvalCounter
                | StatementKind::BackwardIncompatibleDropHint { .. }
                | StatementKind::Nop => {}
            }
        }
    }
    modified
}

#[cfg(test)]
mod tests {
    use rustc_index::{IndexVec, bit_set::DenseBitSet};
    use rustc_middle::mir::BasicBlock;
    use smallvec::{SmallVec, smallvec};

    use super::natural_loop;

    #[test]
    fn collects_a_natural_loop_from_its_latch() {
        let header = BasicBlock::from_usize(1);
        let latch = BasicBlock::from_usize(3);
        let predecessors: IndexVec<BasicBlock, SmallVec<[BasicBlock; 2]>> = [
            smallvec![],
            smallvec![BasicBlock::from_usize(0), latch],
            smallvec![header],
            smallvec![BasicBlock::from_usize(2)],
        ]
        .into_iter()
        .collect();
        let blocks = natural_loop(header, &[latch], |block| &predecessors[block], 4);
        let mut expected = DenseBitSet::new_empty(4);
        expected.insert_range(header..=latch);

        assert_eq!(blocks, expected);
    }
}
