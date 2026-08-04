use index_vec::IndexVec as Vec;

mod context;
mod def;
mod smt;
mod vcgen;

pub use context::{Alloc, Context};
pub use def::{
    Expr, ExprDef, Name, Op, Sort, SortDef, Stmt, StmtDef, Sym, SymDef, SymDefInterned, Uop, Program
};
pub use smt::{format_expr, smt};
pub use vcgen::{VerificationCondition, subst, vc, wp};
