use example::{
    Intern, Context, Expr, ExprDef::*, Op::*, Program, Sort, SortDef::*, Stmt, StmtDef::*, Sym,
    SymDef, smt, vc,
};

use std::{
    io::Write,
    process::{Command, Stdio},
};

fn add_assign(ctxt: &mut Context, var: Sym, amount: i32) -> Stmt {
    let lhs = ctxt.intern(Sym(var));
    let rhs = ctxt.intern(Const(amount));
    let value = ctxt.intern(Binary { op: Add, lhs, rhs });
    ctxt.intern(Assign { var, def: value })
}

fn positive(ctxt: &mut Context, var: Sym) -> Expr {
    let lhs = ctxt.intern(Sym(var));
    let zero = ctxt.intern(Const(0));
    ctxt.intern(Binary { op: Gt, lhs, rhs: zero })
}

fn conditional_add(
    ctxt: &mut Context,
    condition_var: Sym,
    target_var: Sym,
    then_amount: i32,
    else_amount: i32,
) -> Stmt {
    let cond = positive(ctxt, condition_var);
    let then_branch = add_assign(ctxt, target_var, then_amount);
    let else_branch = add_assign(ctxt, target_var, else_amount);

    ctxt.intern(If { cond, then_branch, else_branch })
}

fn test_program(ctxt: &mut Context) -> Program {
    let int: Sort = ctxt.intern(Int);

    let x = ctxt.intern(SymDef { name: "x", sort: int });
    let y = ctxt.intern(SymDef { name: "y", sort: int });
    let ret = ctxt.intern(SymDef { name: "ret", sort: int });
    let result = ctxt.intern(Sym(ret));

    let zero = ctxt.intern(Const(0));
    let four = ctxt.intern(Const(4));
    let six = ctxt.intern(Const(6));

    let initialize = ctxt.intern(Assign { var: ret, def: zero });
    let first_if = conditional_add(ctxt, x, ret, 1, 2);
    let second_if = conditional_add(ctxt, y, ret, 3, 4);
    let remaining = ctxt.intern(Seq { first: first_if, second: second_if });
    let body = ctxt.intern(Seq { first: initialize, second: remaining });

    let lower_bound = ctxt.intern(Binary { op: Ge, lhs: result, rhs: four });
    let upper_bound = ctxt.intern(Binary { op: Le, lhs: result, rhs: six });

    let requires = Box::new([]);
    let ensures = ctxt.intern(Binary { op: And, lhs: lower_bound, rhs: upper_bound });

    Program { body, requires, ensures }
}

fn z3(script: &str) -> String {
    let mut child = Command::new("z3")
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Z3 must be installed and available on PATH");

    child
        .stdin
        .take()
        .expect("Z3 stdin must be piped")
        .write_all(script.as_bytes())
        .expect("failed to send the SMT script to Z3");

    let output = child.wait_with_output().expect("failed to wait for Z3");
    let stdout = String::from_utf8(output.stdout).expect("Z3 stdout must be UTF-8");
    let stderr = String::from_utf8(output.stderr).expect("Z3 stderr must be UTF-8");

    stdout
        .lines()
        .next()
        .unwrap_or_else(|| {
            panic!("Z3 produced no result (status: {}, stderr: {stderr})", output.status)
        })
        .to_owned()
}

#[test]
fn the_result_is_always_between_four_and_six() {
    let mut ctxt = Context::default();
    let program = test_program(&mut ctxt);
    let verification = vc(&mut ctxt, program);
    let script = smt(&ctxt, verification);

    assert_eq!(z3(&script), "unsat");
}
