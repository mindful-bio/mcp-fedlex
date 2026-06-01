//! mcp-reader - zustandsloser MCP-Reader (CQRS-Leseseite).
//!
//! Binary-Entrypoint. Die Bausteine (Auth, Quota) liegen in der Lib.

fn main() {
    println!("mcp-reader: SSE/JSON-RPC-Transport ueber Auth, Quota, Registry, Provenance-Gate");
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_builds() {
        assert_eq!(2 + 2, 4);
    }
}
