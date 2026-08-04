use example::{
    Alloc, Context, Expr, ExprDef::*, Op::*, Program, Sort, SortDef::*, Stmt, StmtDef::*, Sym,
    SymDef, smt, vc,
};

use std::{
    io::Write,
    process::{Command, Stdio},
};

fn add_assign(ctxt: &mut Context, var: Sym, amount: i32) -> Stmt {
    let lhs = ctxt.alloc(Sym(var));
    let rhs = ctxt.alloc(Const(amount));
    let value = ctxt.alloc(Binary { op: Add, lhs, rhs });
    ctxt.alloc(Assign { var, def: value })
}

fn positive(ctxt: &mut Context, var: Sym) -> Expr {
    let lhs = ctxt.alloc(Sym(var));
    let zero = ctxt.alloc(Const(0));
    ctxt.alloc(Binary { op: Gt, lhs, rhs: zero })
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

    ctxt.alloc(If { cond, then_branch, else_branch })
}

fn test_program(ctxt: &mut Context) -> Program {
    let int: Sort = ctxt.alloc(Int);

    let x = ctxt.alloc(SymDef { name: "x", sort: int });
    let y = ctxt.alloc(SymDef { name: "y", sort: int });
    let ret = ctxt.alloc(SymDef { name: "ret", sort: int });
    let result = ctxt.alloc(Sym(ret));

    let zero = ctxt.alloc(Const(0));
    let four = ctxt.alloc(Const(4));
    let six = ctxt.alloc(Const(6));

    let initialize = ctxt.alloc(Assign { var: ret, def: zero });
    let first_if = conditional_add(ctxt, x, ret, 1, 2);
    let second_if = conditional_add(ctxt, y, ret, 3, 4);
    let remaining = ctxt.alloc(Seq { first: first_if, second: second_if });
    let body = ctxt.alloc(Seq { first: initialize, second: remaining });

    let lower_bound = ctxt.alloc(Binary { op: Ge, lhs: result, rhs: four });
    let upper_bound = ctxt.alloc(Binary { op: Le, lhs: result, rhs: six });

    let requires = Box::new([]);
    let ensures = ctxt.alloc(Binary { op: And, lhs: lower_bound, rhs: upper_bound });

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
