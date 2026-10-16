# JSON array pipeline

This project proves the `0.08` JSON ingestion gain: a bounded JSON array is
converted into a typed `List[F64]` and iterated by Seloryn source. It requires
the `data` read capability and returns the deterministic total `11.0`.
