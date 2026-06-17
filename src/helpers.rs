/// C из n по k
pub fn comb(n: u32, k: u32) -> f64 {
    if k > n {
        return 0.0;
    }
    let k = k.min(n - k);
    if k == 0 {
        return 1.0;
    }

    let mut result = 1.0;
    for i in 0..k {
        result *= (n - i) as f64;
        result /= (i + 1) as f64;
    }
    result
}

/// считает префиксные суммы
pub fn prefix_sums<T>(costs: &[T]) -> Vec<f64>
where
    T: Into<f64> + Copy,
{
    let mut prefix = Vec::with_capacity(costs.len() + 1);
    prefix.push(0.0);
    let mut acc = 0.0;
    for &cost in costs {
        acc += cost.into();
        prefix.push(acc);
    }
    prefix
}
