# Geospatial Queries (GeoSPARQL)

pg_ripple implements the [GeoSPARQL 1.1](https://docs.ogc.org/is/22-047r1/22-047r1.html) query function vocabulary for geographic data, delegating geometry computation to [PostGIS](https://postgis.net). You store WKT literals as triple objects, and SPARQL filter functions like `geof:sfWithin` and `geof:sfIntersects` resolve them against PostGIS at query time — without any extra schema work on your part.

---

## What you get

| Capability | Function family | Notes |
|---|---|---|
| Topological filters | `geof:sfWithin`, `geof:sfIntersects`, `geof:sfContains`, `geof:sfTouches`, … | Simple Features 1.x |
| Distance | `geof:distance` | Returns metres for geographic CRS |
| Constructive operations | `geof:buffer`, `geof:convexHull`, `geof:envelope`, `geof:union`, `geof:intersection` | Returns a geometry literal |
| Accessor functions | `geof:asWKT`, `geof:srid` | Inspection |

All of these compile to PostGIS function calls in the generated SQL. You inherit PostGIS's spatial index support automatically when you register a geometry index on the relevant VP table.

---

## Storing geometries

Geometries live as Well-Known Text (WKT) literals on a geometry predicate of your choice. The conventional predicate is `locn:geometry`:

```sql
SELECT pg_ripple.load_turtle($TTL$
@prefix ex:   <https://example.org/> .
@prefix locn: <https://www.w3.org/ns/locn#> .

ex:berlin   locn:geometry "POINT(13.404954 52.520008)" .
ex:munich   locn:geometry "POINT(11.5820  48.1351)" .
ex:bavaria  locn:geometry "POLYGON((9.0 47.0, 13.8 47.0, 13.8 50.6, 9.0 50.6, 9.0 47.0))" .
$TTL$);
```

For better performance, create a PostGIS geometry index on the VP table for `locn:geometry` (one-time, per predicate):

```sql
SELECT pg_ripple.create_spatial_index('<https://www.w3.org/ns/locn#geometry>');
```

---

## Querying

### Find every point inside a polygon

```sparql
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>
PREFIX locn: <https://www.w3.org/ns/locn#>

SELECT ?city WHERE {
    ?city locn:geometry ?g .
    FILTER(geof:sfWithin(?g,
        "POLYGON((9.0 47.0, 13.8 47.0, 13.8 50.6, 9.0 50.6, 9.0 47.0))"))
}
```

### Distance-bounded nearest-neighbour

```sparql
SELECT ?city ?d WHERE {
    ?city locn:geometry ?g .
    BIND(geof:distance(?g, "POINT(11.5820 48.1351)") AS ?d)
    FILTER(?d < 200000)        # within 200 km of Munich
}
ORDER BY ?d
```

### Buffer + intersect (constructive)

```sparql
SELECT ?city WHERE {
    ?city locn:geometry ?g .
    FILTER(geof:sfIntersects(?g,
        geof:buffer("POINT(11.5820 48.1351)", 50000)))   # 50 km around Munich
}
```

---

## Coordinate reference systems

GeoSPARQL literals can carry a CRS as an IRI suffix:

```turtle
ex:berlin locn:geometry
    "<http://www.opengis.net/def/crs/EPSG/0/4326> POINT(13.4049 52.5200)" .
```

When the CRS is omitted, pg_ripple uses WGS84 (EPSG:4326) as the default. This matches the GeoSPARQL 1.1 default.

---

## What is *not* implemented

- Egenhofer (`geof:eh*`) and RCC8 (`geof:rcc8*`) topological functions are not yet wired up.
- The `gml:Geometry` literal datatype is not parsed (only `geo:wktLiteral`).
- DE-9IM matrix queries are not exposed.

If you need any of these, file an issue — most are a thin wrapper over the corresponding PostGIS function.

---

## See also

- [GeoSPARQL function catalog](../reference/geosparql.md)
- [PostGIS documentation](https://postgis.net/docs/)
