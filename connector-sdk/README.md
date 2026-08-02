# connector-sdk

The interface between an external source of documents and
`capsule-compiler` (Phase 9's "connector SDK").

```rust
let connector = connector_sdk::FilesystemConnector::new("./sources");
for source_ref in connector.list()? {
    let source = connector_sdk::ingest(&connector, &source_ref)?;
    let report = capsule_compiler::compile(&source, &options);
    // ...
}
```

```rust
pub trait SourceConnector {
    fn list(&self) -> Result<Vec<SourceRef>, ConnectorError>;
    fn fetch(&self, source_ref: &SourceRef) -> Result<Vec<u8>, ConnectorError>;
}
```

`FilesystemConnector` is the one connector implemented here — real,
tested, no network dependency: it walks a directory tree and recognizes
`.txt`, `.md`/`.markdown`, `.html`/`.htm`, and `.pdf` files. `ingest()`
dispatches to the matching `capsule-compiler` adapter, so implementing
`SourceConnector` is enough to plug a new source into compilation.

**Named connectors for GitHub, SharePoint, Confluence, object storage, a
generic API, and a database are not implemented.** Each needs real
credentials, a real endpoint, and integration testing against that live
service — this environment can't do that honestly, and shipping
untested network code against those APIs would be worse than not
shipping it. The trait is the deliverable for those: implement
`SourceConnector` against a real client and `capsule-compiler`,
`capsule-registry`, and everything downstream already work, exactly as
`FilesystemConnector` demonstrates.
