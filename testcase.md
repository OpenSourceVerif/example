```rs
// requires q
// ensures p(result)
fn f(x: int, y:int) -> int{
    let mut ret = 0;
    if x > 0 {
        ret += 1;
    } else {
        ret += 2;
    }

    if y > 0 {
        ret += 3;
    } else {
        ret += 4;
    }

    ret
}
```

// q => wp(body, p(result))