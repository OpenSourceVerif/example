use index_vec::define_index_type;

define_index_type! { pub struct Expr = u32; }
define_index_type! { pub struct Stmt = u32; }
define_index_type! { pub struct Sym = u32; }
define_index_type! { pub struct Sort = u32; }
define_index_type! { pub struct Name = u32; }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StmtDef {
    Skip,
    If { cond: Expr, then_branch: Stmt, else_branch: Stmt },
    Assign { var: Sym, def: Expr },
    Seq { first: Stmt, second: Stmt },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uop {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExprDef {
    Sym(Sym),
    Const(i32),
    Binary { op: Op, lhs: Expr, rhs: Expr },
    Unary { op: Uop, expr: Expr },
    Call { func: Sym, arg: Expr },
}

#[derive(Debug, Clone, Copy)]
pub struct SymDef<'c> {
    pub name: &'c str,
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymDefInterned {
    pub name: Name,
    pub sort: Sort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SortDef {
    Int,
    Bool,
    Arrow(Sort, Sort),
}

pub struct Program {
    pub body: Stmt,
    pub requires: Box<[Expr]>,
    pub ensures: Expr,
}
