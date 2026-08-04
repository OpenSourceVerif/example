use example::{Context, Expr, Program, Stmt, Sym, smt};

use std::{
    io::Write,
    process::{Command, Stdio},
};

fn add_assign(ctxt: &mut Context, var: Sym, amount: i32) -> Stmt {
    let lhs = ctxt.sym(var);
    let rhs = ctxt.int_lit(amount);
    let value = ctxt.add(lhs, rhs);
    ctxt.assign(var, value)
}

fn is_positive(ctxt: &mut Context, var: Sym) -> Expr {
    let lhs = ctxt.sym(var);
    let zero = ctxt.int_lit(0);
    ctxt.gt(lhs, zero)
}

fn conditional_add(
    ctxt: &mut Context,
    condition_var: Sym,
    target_var: Sym,
    then_amount: i32,
    else_amount: i32,
) -> Stmt {
    let cond = is_positive(ctxt, condition_var);
    let then_branch = add_assign(ctxt, target_var, then_amount);
    let else_branch = add_assign(ctxt, target_var, else_amount);

    ctxt.if_(cond, then_branch, else_branch)
}

fn test_program(ctxt: &mut Context) -> Program {
    let int = ctxt.int_sort();

    let x = ctxt.symbol("x", int);
    let y = ctxt.symbol("y", int);
    let ret = ctxt.symbol("ret", int);
    let result = ctxt.sym(ret);

    let zero = ctxt.int_lit(0);
    let four = ctxt.int_lit(4);
    let six = ctxt.int_lit(6);

    let initialize = ctxt.assign(ret, zero);
    let first_if = conditional_add(ctxt, x, ret, 1, 2);
    let second_if = conditional_add(ctxt, y, ret, 3, 4);
    let remaining = ctxt.seq(first_if, second_if);
    let body = ctxt.seq(initialize, remaining);

    let lower_bound = ctxt.ge(result, four);
    let upper_bound = ctxt.le(result, six);

    let requires = Box::new([]);
    let ensures = ctxt.and(lower_bound, upper_bound);

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
    let verification = ctxt.vc(program);
    let script = smt(&ctxt, verification);

    println!("{:}", script);

    assert_eq!(z3(&script), "unsat");
}
