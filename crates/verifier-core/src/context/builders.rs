//! Typed constructors for interned terms and sorts.

use crate::{
    Context, DefStore, Field, Fields, Intern, Op, Sort, SortDef, Sym, Term, TermDef, TermKind, Uop,
};

macro_rules! binary_builders {
    ($($name:ident: $op:ident;)*) => {$(
        pub fn $name(&mut self, lhs: Term, rhs: Term) -> Term {
            self.binary(Op::$op, lhs, rhs)
        }
    )*};
}

impl Context {
    pub fn term_sort(&self, term: Term) -> Sort {
        self.get(term).sort
    }

    fn term(&mut self, sort: Sort, kind: TermKind<'_>) -> Term {
        self.intern(TermDef { sort, kind })
    }

    pub fn param(&mut self, index: u32, sort: Sort) -> Term {
        self.term(sort, TermKind::Param(index))
    }

    pub fn sym(&mut self, sym: Sym) -> Term {
        self.term(self.get(sym).sort, TermKind::Sym(sym))
    }

    pub fn int_lit(&mut self, value: i128) -> Term {
        let sort = self.int_sort();
        self.term(sort, TermKind::Const(value))
    }

    pub fn bool_lit(&mut self, value: bool) -> Term {
        let sort = self.bool_sort();
        self.term(sort, TermKind::Bool(value))
    }

    pub fn unit(&mut self) -> Term {
        let sort = self.unit_sort();
        self.term(sort, TermKind::Unit)
    }

    pub fn tuple(&mut self, fields: &[Term]) -> Term {
        if fields.is_empty() {
            return self.unit();
        }

        let sorts: Vec<_> = fields.iter().map(|field| self.term_sort(*field)).collect();
        let sort = self.tuple_sort(&sorts);
        self.term(sort, TermKind::Tuple(Fields::new(fields)))
    }

    pub fn proj(&mut self, tuple: Term, field: impl Into<Field>) -> Term {
        let field = field.into();
        let data = self.get(tuple);
        if let TermKind::Tuple(fields) = data.kind {
            return fields[field];
        }

        let sort = match self.get(data.sort) {
            SortDef::Tuple(fields) => fields[field],
            _ => panic!("projection from non-tuple term"),
        };
        self.term(sort, TermKind::Proj { tuple, field })
    }

    pub fn call(&mut self, func: Sym, arg: Term) -> Term {
        let function_sort = self.get(func).sort;
        let (domain, range) = match self.get(function_sort) {
            SortDef::Arrow(domain, range) => (domain, range),
            _ => panic!("call of non-function symbol"),
        };
        assert_eq!(self.term_sort(arg), domain, "function argument sort mismatch");
        self.term(range, TermKind::Call { func, arg })
    }

    pub fn binary(&mut self, op: Op, lhs: Term, rhs: Term) -> Term {
        let lhs_sort = self.term_sort(lhs);
        let rhs_sort = self.term_sort(rhs);
        let int = self.int_sort();
        let bool = self.bool_sort();
        let sort = match op {
            Op::Add | Op::Sub | Op::Mul => {
                assert_eq!((lhs_sort, rhs_sort), (int, int), "integer operation sort mismatch");
                int
            }
            Op::Eq | Op::Ne => {
                assert_eq!(lhs_sort, rhs_sort, "equality sort mismatch");
                bool
            }
            Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                assert_eq!((lhs_sort, rhs_sort), (int, int), "comparison sort mismatch");
                bool
            }
            Op::And | Op::Or | Op::Implies => {
                assert_eq!((lhs_sort, rhs_sort), (bool, bool), "boolean operation sort mismatch");
                bool
            }
        };
        self.term(sort, TermKind::Binary { op, lhs, rhs })
    }

    binary_builders! {
        add: Add;
        sub: Sub;
        mul: Mul;
        eq: Eq;
        ne: Ne;
        lt: Lt;
        le: Le;
        gt: Gt;
        ge: Ge;
        and: And;
        or: Or;
        implies: Implies;
    }

    pub fn unary(&mut self, op: Uop, expr: Term) -> Term {
        let operand_sort = self.term_sort(expr);
        let int = self.int_sort();
        let bool = self.bool_sort();
        let sort = match op {
            Uop::Not => {
                assert_eq!(operand_sort, bool, "boolean negation sort mismatch");
                bool
            }
            Uop::Neg => {
                assert_eq!(operand_sort, int, "integer negation sort mismatch");
                int
            }
        };
        self.term(sort, TermKind::Unary { op, expr })
    }

    pub fn not(&mut self, expr: Term) -> Term {
        self.unary(Uop::Not, expr)
    }
    pub fn neg(&mut self, expr: Term) -> Term {
        self.unary(Uop::Neg, expr)
    }

    pub fn int_sort(&mut self) -> Sort {
        self.intern(SortDef::Int)
    }
    pub fn bool_sort(&mut self) -> Sort {
        self.intern(SortDef::Bool)
    }
    pub fn unit_sort(&mut self) -> Sort {
        self.tuple_sort(&[])
    }
    pub fn tuple_sort(&mut self, fields: &[Sort]) -> Sort {
        self.intern(SortDef::Tuple(Fields::new(fields)))
    }
    pub fn arrow(&mut self, domain: Sort, range: Sort) -> Sort {
        self.intern(SortDef::Arrow(domain, range))
    }
}
