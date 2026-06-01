//! mcp-ingest - event-getriebener Writer (CQRS-Schreibseite).
//!
//! In M0 nur ein Platzhalter-Entrypoint. Ab M8 trägt die Bibliothek
//! ([`mcp_ingest`]) den resilienten Schreibpfad (Consumer, DLQ, Outbox, Writer).

fn main() {
    println!("mcp-ingest: M8 (Writer, Embedding-Outbox, DLQ)");
}

#[cfg(test)]
mod tests {
    #[test]
    fn smoke_builds() {
        assert_eq!(2 + 2, 4);
    }
}
