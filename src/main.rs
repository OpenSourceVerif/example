use index_vec::IndexVec as Vec;

mod context;
mod def;
mod vcgen;

pub use context::{Alloc, Context};
pub use def::{Expr, ExprDef, Op, Stmt, StmtDef, Uop, Var};
pub use vcgen::{subst, wp};

fn main() {
    let mut ctxt = Context::default();

    let var = ctxt.alloc(Box::<str>::from("x"));
    let value = ctxt.alloc(ExprDef::Const(1));
    let first = ctxt.alloc(StmtDef::Assign { var, def: value });
    let program = ctxt.alloc(StmtDef::Seq(first, first));
    let postcondition = ctxt.alloc(ExprDef::Var(var));

    let precondition = wp(&mut ctxt, program, postcondition);
    println!("{:?}", ctxt[precondition]);
}
