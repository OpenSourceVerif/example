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
    If { cond: Expr, then_branch: Stmt, else_branch: Stmt },
    Assign { var: Var, def: Expr },
    Seq(Stmt, Stmt),
}

#[derive(Debug, Clone, Copy)]
pub enum Op {
    Add,
    And,
}

#[derive(Debug, Clone, Copy)]
pub enum Uop {
    Not,
}

#[derive(Debug, Clone, Copy)]
pub enum ExprDef {
    Var(Var),
    Const(i32),
    Binary { lhs: Expr, rhs: Expr, op: Op },
    Unary { op: Uop, expr: Expr },
}
