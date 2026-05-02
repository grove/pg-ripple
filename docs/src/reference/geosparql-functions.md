# GeoSPARQL Functions

> **SC13-03 (v0.86.0)**: this page documents the implementation status of all GeoSPARQL 1.1 functions in pg_ripple.

pg_ripple implements the GeoSPARQL 1.1 extension (OGC, 2022) via the PostGIS integration layer.
Geometry literals are stored as WKT strings with the `geo:wktLiteral` datatype.

---

## Topological Relations

GeoSPARQL topological relation functions operate on geometry literals and return `xsd:boolean`.

| Function | Prefix | Status | Notes |
|---|---|---|---|
| `geo:sfEquals` | geo: | ✅ Implemented | PostGIS `ST_Equals` |
| `geo:sfDisjoint` | geo: | ✅ Implemented | PostGIS `ST_Disjoint` |
| `geo:sfIntersects` | geo: | ✅ Implemented | PostGIS `ST_Intersects` |
| `geo:sfTouches` | geo: | ✅ Implemented | PostGIS `ST_Touches` |
| `geo:sfCrosses` | geo: | ✅ Implemented | PostGIS `ST_Crosses` |
| `geo:sfWithin` | geo: | ✅ Implemented | PostGIS `ST_Within` |
| `geo:sfContains` | geo: | ✅ Implemented | PostGIS `ST_Contains` |
| `geo:sfOverlaps` | geo: | ✅ Implemented | PostGIS `ST_Overlaps` |
| `geo:ehEquals` | geo: | ✅ Implemented | Egenhofer equals |
| `geo:ehDisjoint` | geo: | ✅ Implemented | Egenhofer disjoint |
| `geo:ehMeet` | geo: | ✅ Implemented | Egenhofer meet |
| `geo:ehOverlap` | geo: | ✅ Implemented | Egenhofer overlap |
| `geo:ehCovers` | geo: | ✅ Implemented | Egenhofer covers |
| `geo:ehCoveredBy` | geo: | ✅ Implemented | Egenhofer covered by |
| `geo:ehInside` | geo: | ✅ Implemented | Egenhofer inside |
| `geo:ehContains` | geo: | ✅ Implemented | Egenhofer contains |
| `geo:rcc8dc` | geo: | ✅ Implemented | RCC8 DC |
| `geo:rcc8ec` | geo: | ✅ Implemented | RCC8 EC |
| `geo:rcc8po` | geo: | ✅ Implemented | RCC8 PO |
| `geo:rcc8tppi` | geo: | ✅ Implemented | RCC8 TPPi |
| `geo:rcc8tpp` | geo: | ✅ Implemented | RCC8 TPP |
| `geo:rcc8ntppi` | geo: | ✅ Implemented | RCC8 NTPPi |
| `geo:rcc8ntpp` | geo: | ✅ Implemented | RCC8 NTPP |
| `geo:rcc8eq` | geo: | ✅ Implemented | RCC8 EQ |

---

## Non-topological Functions

| Function | Status | Returns | Notes |
|---|---|---|---|
| `geo:distance` | ✅ Implemented | `xsd:double` | PostGIS `ST_Distance`; unit = degrees (EPSG:4326) |
| `geo:buffer` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Buffer` |
| `geo:convexHull` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_ConvexHull` |
| `geo:intersection` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Intersection` |
| `geo:union` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Union` |
| `geo:difference` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Difference` |
| `geo:symDifference` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_SymDifference` |
| `geo:envelope` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Envelope` |
| `geo:boundary` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_Boundary` |
| `geo:area` | ✅ Implemented | `xsd:double` | PostGIS `ST_Area`; unit = square degrees |
| `geo:length` | ✅ Implemented | `xsd:double` | PostGIS `ST_Length` |
| `geo:asWKT` | ✅ Implemented | `geo:wktLiteral` | PostGIS `ST_AsText` |
| `geo:asGeoJSON` | ✅ Implemented | `xsd:string` | PostGIS `ST_AsGeoJSON` |

---

## Known Gaps

| Function | Status | Reason |
|---|---|---|
| `geo:metricArea` | ⏳ Planned | Requires CRS-aware unit conversion via `GEOGRAPHY` type |
| `geo:metricLength` | ⏳ Planned | Requires CRS-aware unit conversion |
| `geo:metricPerimeter` | ⏳ Planned | Requires CRS-aware unit conversion |
| `geo:metricDistance` | ⏳ Planned | Requires CRS-aware unit conversion |
| DGGS functions | 🔜 Future | Discrete Global Grid Systems (DGGS) not yet implemented |

---

## Enabling GeoSPARQL

GeoSPARQL requires the PostGIS extension:

```sql
CREATE EXTENSION postgis;
CREATE EXTENSION pg_ripple CASCADE;
```

See [GeoSPARQL + PostGIS Integration](../../blog/geosparql-postgis-spatial.md) for a full tutorial.
