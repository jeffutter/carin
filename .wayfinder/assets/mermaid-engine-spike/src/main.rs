// PROTOTYPE spike, wipe me.
use rquickjs::{Context, Runtime};
use std::time::Instant;

const PELE: &str = include_str!(env!("PELE_JS"));

fn main() {
    let t0 = Instant::now();
    let rt = Runtime::new().unwrap();
    let ctx = Context::full(&rt).unwrap();
    ctx.with(|c| { c.eval::<(), _>(PELE).unwrap(); });
    println!("init (engine + load Pele): {:?}", t0.elapsed());

    let src = std::env::args().nth(1).unwrap_or("flowchart LR\n A[Start] --> B{Ok?}\n B -->|yes| C[Done]\n B -->|no| A".into());
    let js = format!("pele.render({}, {{links:false, images:false}}).svg", serde_str(&src));
    let mut last = String::new();
    for i in 0..5 {
        let t = Instant::now();
        let out: Result<String, _> = ctx.with(|c| c.eval::<String, _>(js.as_str()));
        match out {
            Ok(s) => { println!("render #{i}: {:?} ({} bytes)", t.elapsed(), s.len()); last = s; }
            Err(e) => { println!("render error: {e:?}"); break; }
        }
    }
    std::fs::write("out.svg", last).unwrap();
    println!("rss kB: {}", std::fs::read_to_string("/proc/self/status").unwrap().lines().find(|l| l.starts_with("VmRSS")).unwrap());
}

fn serde_str(s: &str) -> String {
    let mut o = String::from("\"");
    for ch in s.chars() { match ch { '"' => o.push_str("\\\""), '\\' => o.push_str("\\\\"), '\n' => o.push_str("\\n"), c => o.push(c) } }
    o.push('"'); o
}
