fn main() {
    println!(
        "{}",
        serde_json::Value::Array(emu198x_fleet_web::catalogue())
    );
}
