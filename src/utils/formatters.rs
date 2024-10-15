pub fn format_sol(value: f64) -> String {
    let s = format!("{:.4}", value);
    let s = s.trim_end_matches('0');
    s.trim_end_matches('.').to_string()
}
