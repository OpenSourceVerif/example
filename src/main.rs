use index_vec::IndexVec as Vec;

mod context;
mod def;
mod render;
mod vcgen;

pub use context::{Alloc, Context};
pub use def::{Expr, ExprDef, Op, Stmt, StmtDef, Uop, Var};
pub use render::format;
pub use vcgen::{subst, wp};

fn main() {}
