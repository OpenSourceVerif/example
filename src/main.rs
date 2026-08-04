use index_vec::IndexVec as Vec;

mod context;
mod def;
mod smt;
mod string_interner;
mod vcgen;

pub use context::{Alloc, Context};
pub use def::{Expr, ExprDef, Op, Sort, SortDef, Stmt, StmtDef, Sym, SymDef, Uop, SymDefInterned, Name};
pub use smt::format_expr;
pub use vcgen::{subst, wp, VerificationCondition};

fn main() {
    
}
