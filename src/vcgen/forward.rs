use std::collections::HashMap;

use crate::{Context, Expr, ExprDef, Intern, Program, Stmt, StmtDef, Sym, vcgen::VC};

#[derive(Clone)]
struct State {
    /// Every value in the store is expressed in terms of the initial symbols.
    store: HashMap<Sym, Expr>,
    /// Path condition.
    fact: Expr,
}

impl State {
    fn initial(ctxt: &mut Context) -> Self {
        Self { fact: ctxt.bool_lit(true), store: HashMap::new() }
    }
}

impl Context {
    /// Generate a verification condition by executing `program` forwards with
    /// symbolic values.
    ///
    /// Assignments update a symbolic store, conditionals fork the current path,
    /// and assertions create obligations at the point where they are reached.
    /// Each surviving path also creates an obligation for the postcondition.
    pub fn vc_by_forward(&mut self, Program { body, requires, ensures }: Program) -> VC {
        let mut obligations = Vec::new();
        let initial_state = State::initial(self);
        let final_states = self.execute(body, initial_state, &mut obligations);

        obligations.extend(final_states.into_iter().map(|state| {
            let ensures = self.eval(ensures, &state.store);
            self.implies(state.fact, ensures)
        }));

        let joined_obligations = self.conjoin(obligations);
        self.implies(requires, joined_obligations)
    }

    fn execute(&mut self, stmt: Stmt, mut state: State, obligations: &mut Vec<Expr>) -> Vec<State> {
        match self.get(stmt) {
            StmtDef::Skip => vec![state],
            StmtDef::Assign { var, def } => {
                let value = self.eval(def, &state.store);
                state.store.insert(var, value);
                vec![state]
            }
            StmtDef::Seq { first, second } => self
                .execute(first, state, obligations)
                .into_iter()
                .flat_map(|state| self.execute(second, state, obligations))
                .collect(),
            StmtDef::If { cond, then_branch, else_branch } => {
                let cond = self.eval(cond, &state.store);

                let mut then_state = state.clone();
                then_state.fact = self.and(then_state.fact, cond);

                let not_cond = self.not(cond);
                state.fact = self.and(state.fact, not_cond);

                let mut final_states = self.execute(then_branch, then_state, obligations);
                final_states.extend(self.execute(else_branch, state, obligations));
                final_states
            }
            StmtDef::Assert(assertion) => {
                let assertion = self.eval(assertion, &state.store);
                let obligation = self.implies(state.fact, assertion);
                obligations.push(obligation);

                // Execution after a successful assertion may use it as an assumption.
                state.fact = self.and(state.fact, assertion);
                vec![state]
            }
        }
    }

    /// Evaluate an expression in the current symbolic store. Store entries are
    /// already snapshots in terms of entry-state symbols, so a symbol lookup
    /// deliberately does not recursively re-evaluate the returned expression.
    fn eval(&mut self, expr: Expr, store: &HashMap<Sym, Expr>) -> Expr {
        match self.get(expr) {
            ExprDef::Sym(sym) => store.get(&sym).copied().unwrap_or(expr),
            ExprDef::Const(_) | ExprDef::Bool(_) => expr,
            ExprDef::Binary { op, lhs, rhs } => {
                let lhs = self.eval(lhs, store);
                let rhs = self.eval(rhs, store);
                self.binary(op, lhs, rhs)
            }
            ExprDef::Unary { op, expr } => {
                let expr = self.eval(expr, store);
                self.unary(op, expr)
            }
            ExprDef::Call { func, arg } => {
                let arg = self.eval(arg, store);
                self.call(func, arg)
            }
        }
    }

    fn conjoin(&mut self, expressions: Vec<Expr>) -> Expr {
        let mut expressions = expressions.into_iter();
        let Some(first) = expressions.next() else {
            return self.bool_lit(true);
        };

        expressions.fold(first, |result, expression| self.and(result, expression))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Context, Program};

    #[test]
    fn assignments_preserve_snapshots_of_old_values() {
        let mut context = Context::default();
        let int = context.int_sort();
        let x = context.symbol("x", int);
        let y = context.symbol("y", int);

        let x_expr = context.sym(x);
        let y_expr = context.sym(y);
        let zero = context.int_lit(0);
        let one = context.int_lit(1);
        let save_y = context.assign(x, y_expr);
        let overwrite_y = context.assign(y, one);
        let body = context.seq(save_y, overwrite_y);
        let ensures = context.eq(x_expr, y_expr);
        let requires = context.gt(y_expr, zero);

        let verification = context.vc_by_forward(Program { body, requires, ensures });
        let expected = context.eq(y_expr, one);
        assert_eq!(verification, expected);
    }

    #[test]
    fn conditionals_create_one_postcondition_obligation_per_path() {
        let mut context = Context::default();
        let int = context.int_sort();
        let bool_ = context.bool_sort();
        let x = context.symbol("x", int);
        let c = context.symbol("c", bool_);
        let true_ = context.bool_lit(true);

        let c_expr = context.sym(c);
        let x_expr = context.sym(x);
        let zero = context.int_lit(0);
        let one = context.int_lit(1);
        let two = context.int_lit(2);
        let then_branch = context.assign(x, one);
        let else_branch = context.assign(x, two);
        let body = context.if_(c_expr, then_branch, else_branch);
        let ensures = context.gt(x_expr, zero);

        let verification = context.vc_by_forward(Program { body, requires: true_, ensures });

        let then_postcondition = context.gt(one, zero);
        let then_obligation = context.implies(c_expr, then_postcondition);
        let not_c = context.not(c_expr);
        let else_postcondition = context.gt(two, zero);
        let else_obligation = context.implies(not_c, else_postcondition);
        let expected = context.and(then_obligation, else_obligation);
        assert_eq!(verification, expected);
    }

    #[test]
    fn assertions_create_obligations_and_constrain_the_remaining_path() {
        let mut context = Context::default();
        let int = context.int_sort();
        let x = context.symbol("x", int);
        let true_ = context.bool_lit(true);

        let x_expr = context.sym(x);
        let zero = context.int_lit(0);
        let assertion = context.gt(x_expr, zero);
        let check = context.assert(assertion);
        let overwrite_x = context.assign(x, zero);
        let body = context.seq(check, overwrite_x);
        let ensures = context.gt(x_expr, zero);

        let verification = context.vc_by_forward(Program { body, requires: true_, ensures });

        let evaluated_postcondition = context.gt(zero, zero);
        let postcondition_obligation = context.implies(assertion, evaluated_postcondition);
        let expected = context.and(assertion, postcondition_obligation);
        assert_eq!(verification, expected);
    }
}
