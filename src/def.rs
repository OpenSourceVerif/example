index_vec::define_index_type! {
    pub struct Expr = u32;
}

index_vec::define_index_type! {
    pub struct Stmt = u32;
}

index_vec::define_index_type! {
    pub struct Var = u32;
}

#[derive(Debug, Clone, Copy)]
pub enum StmtDef {
    Skip,
    If { cond: Expr, then_branch: Stmt, else_branch: Stmt },
    Assign { var: Var, def: Expr },
    Seq { first: Stmt, second: Stmt },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Implies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Uop {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy)]
pub enum ExprDef {
    Var(Var),
    Const(i32),
    Binary { lhs: Expr, rhs: Expr, op: Op },
    Unary { op: Uop, expr: Expr },
    Call { func: Var, arg: Expr },
}
