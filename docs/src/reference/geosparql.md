# GeoSPARQL Reference

pg_ripple v0.25.0 implements a subset of [GeoSPARQL 1.1](https://www.ogc.org/standards/geosparql) using PostGIS as the underlying geometry engine. All geo functions gracefully degrade (returning `false` or `NULL`) when PostGIS is not installed.

## Requirements

| Requirement | Version |
|---|---|
| PostGIS | 2.5+ (3.x recommended) |
| PostGIS SQL extension | Must be installed in the same database |

Check availability:

```sql
SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'st_geomfromtext') AS postgis_available;
```

## Geometry Representation

Geometries are stored as WKT (Well-Known Text) literals in the RDF triple store. Use N-Triples or Turtle syntax to load them:

```sql
SELECT pg_ripple.load_ntriples(
    '<https://geo.example/city/berlin> <https://www.w3.org/ns/locn#geometry>
     "POINT(13.404954 52.520008)" .'
);
```

The `geo:wktLiteral` datatype IRI is stored as a regular literal; the WKT string is passed directly to `ST_GeomFromText()` at query time.

## Topological Relation Functions (FILTER context)

These functions are used in `FILTER(...)` clauses and return `true` or `false`.

| SPARQL IRI | PostGIS equivalent | Description |
|---|---|---|
| `geo:sfIntersects(a, b)` | `ST_Intersects(…)` | Geometries share at least one point |
| `geo:sfContains(a, b)` | `ST_Contains(…)` | A completely contains B |
| `geo:sfWithin(a, b)` | `ST_Within(…)` | A is completely within B |
| `geo:sfOverlaps(a, b)` | `ST_Overlaps(…)` | Geometries overlap |
| `geo:sfTouches(a, b)` | `ST_Touches(…)` | Geometries touch at boundary |
| `geo:sfCrosses(a, b)` | `ST_Crosses(…)` | Geometries cross |
| `geo:sfDisjoint(a, b)` | `ST_Disjoint(…)` | Geometries share no points |
| `geo:sfEquals(a, b)` | `ST_Equals(…)` | Geometries are spatially equal |
| `geo:ehIntersects(a, b)` | `ST_Intersects(…)` | Egenhofer intersection |
| `geo:ehContains(a, b)` | `ST_Contains(…)` | Egenhofer contains |
| `geo:ehCoveredBy(a, b)` | `ST_CoveredBy(…)` | A is covered by B |
| `geo:ehCovers(a, b)` | `ST_Covers(…)` | A covers B |

Namespace prefix: `http://www.opengis.net/def/function/geosparql/`

### Example: Find intersecting geometries

```sparql
PREFIX geo: <http://www.opengis.net/def/function/geosparql/>
SELECT ?city WHERE {
  ?city <https://www.w3.org/ns/locn#geometry> ?geom .
  FILTER(geo:sfIntersects(?geom, "POLYGON((13.0 52.0, 14.0 52.0, 14.0 53.0, 13.0 53.0, 13.0 52.0))"))
}
```

## Measurement Functions (BIND context)

These functions are used in `BIND(...)` clauses and return numeric or WKT values.

| SPARQL IRI | Returns | Description |
|---|---|---|
| `geof:distance(a, b, unit)` | `xsd:double` (metres) | Geodetic distance between two geometries |
| `geof:area(a, unit)` | `xsd:double` (m²) | Surface area of a geometry |
| `geof:boundary(a)` | WKT literal | Boundary geometry as WKT string |

The `unit` argument is accepted for API compatibility but all results are returned in SI base units (metres / square metres).

### Example: Distance query

```sparql
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
SELECT ?city ?dist WHERE {
  ?city <https://www.w3.org/ns/locn#geometry> ?geom .
  BIND(geof:distance(?geom, "POINT(13.404954 52.520008)",
                     <http://www.opengis.net/def/uom/OGC/1.0/metre>) AS ?dist)
  FILTER(?dist < 100000)
}
ORDER BY ?dist
```

## Behaviour When PostGIS Is Absent

When PostGIS is not installed:

- Topological filter functions evaluate to `false` — queries return zero rows
- Measurement functions evaluate to `NULL` — `BIND` variables are unbound
- No `ERROR` is raised; the query completes normally

This allows geospatial queries to be deployed to environments where PostGIS is an optional component.

## Limitations

- 3D geometries (`POINT Z`, `POLYGON Z`) are stored as literals but PostGIS 2D functions will project them to 2D.
- Coordinate reference system (CRS) handling: all geometries are assumed to be in WGS84 (SRID 4326) when passed to the `geography` cast for `geof:distance` and `geof:area`.
- RDF-star quoted triples are not supported as geometry arguments.
