[← Back to Blog Index](README.md)

# GeoSPARQL on PostGIS: Spatial Queries Meet RDF

## Find all sensors within 5km that reported high temperatures — in one SPARQL query

---

Your knowledge graph has sensor locations. PostGIS has spatial indexes. GeoSPARQL is the W3C standard for spatial queries in SPARQL. pg_ripple connects all three.

---

## The Problem

RDF can store coordinates:

```turtle
ex:sensor7 geo:hasGeometry [
  geo:asWKT "POINT(10.75 59.91)"^^geo:wktLiteral
] .
```

But standard SPARQL can't do spatial operations on them. You can't write "find all sensors within 5km of this point" in SPARQL 1.1 — you'd have to extract coordinates, compute distances in application code, and filter manually.

GeoSPARQL adds spatial filter functions to SPARQL. pg_ripple translates them to PostGIS operations, which means your spatial queries use PostgreSQL's GiST indexes and are fast.

---

## GeoSPARQL in pg_ripple

```sparql
SELECT ?sensor ?label ?distance WHERE {
  ?sensor rdf:type ex:Sensor ;
          rdfs:label ?label ;
          geo:hasGeometry ?geom .
  ?geom geo:asWKT ?wkt .

  BIND(geof:distance(?wkt, "POINT(10.75 59.91)"^^geo:wktLiteral, <http://www.opengis.net/def/uom/OGC/1.0/metre>) AS ?distance)
  FILTER(?distance < 5000)
}
ORDER BY ?distance
```

This finds all sensors within 5km of a point in Oslo, sorted by distance.

pg_ripple translates this to:

```sql
SELECT d_label.value AS label,
       ST_Distance(
         ST_GeomFromText(d_wkt.value, 4326)::geography,
         ST_GeomFromText('POINT(10.75 59.91)', 4326)::geography
       ) AS distance
FROM _pg_ripple.vp_{rdf_type} t1
JOIN _pg_ripple.vp_{rdfs_label} t2 ON t2.s = t1.s
JOIN _pg_ripple.vp_{geo_hasGeometry} t3 ON t3.s = t1.s
JOIN _pg_ripple.vp_{geo_asWKT} t4 ON t4.s = t3.o
JOIN _pg_ripple.dictionary d_wkt ON d_wkt.id = t4.o
JOIN _pg_ripple.dictionary d_label ON d_label.id = t2.o
WHERE t1.o = <ex:Sensor encoded>
  AND ST_DWithin(
    ST_GeomFromText(d_wkt.value, 4326)::geography,
    ST_GeomFromText('POINT(10.75 59.91)', 4326)::geography,
    5000
  )
ORDER BY distance;
```

Note the `ST_DWithin` — PostGIS's optimized distance filter that uses the GiST spatial index. This is much faster than computing `ST_Distance` for every row and filtering afterward.

---

## Supported GeoSPARQL Functions

| GeoSPARQL Function | PostGIS Translation |
|--------------------|--------------------|
| `geof:distance` | `ST_Distance` |
| `geof:within` | `ST_Within` |
| `geof:contains` | `ST_Contains` |
| `geof:intersects` | `ST_Intersects` |
| `geof:overlaps` | `ST_Overlaps` |
| `geof:touches` | `ST_Touches` |
| `geof:crosses` | `ST_Crosses` |
| `geof:buffer` | `ST_Buffer` |
| `geof:convexHull` | `ST_ConvexHull` |
| `geof:union` | `ST_Union` |
| `geof:intersection` | `ST_Intersection` |
| `geof:boundary` | `ST_Boundary` |
| `geof:envelope` | `ST_Envelope` |

Each GeoSPARQL function is translated to the equivalent PostGIS function at SQL generation time. WKT literals from the dictionary are decoded and passed to PostGIS operators.

---

## Practical Example: IoT + Spatial + Graph

A city's IoT platform stores sensor data as RDF:

```sparql
SELECT ?sensor ?temp ?location_name WHERE {
  # Spatial: sensors within 2km of city hall
  ?sensor geo:hasGeometry ?geom .
  ?geom geo:asWKT ?wkt .
  FILTER(geof:distance(?wkt,
    "POINT(10.7522 59.9139)"^^geo:wktLiteral,
    <http://www.opengis.net/def/uom/OGC/1.0/metre>) < 2000)

  # Graph: latest temperature reading
  ?sensor ex:latestReading ?reading .
  ?reading ex:temperature ?temp .
  FILTER(?temp > 30)

  # Graph: location name from the spatial hierarchy
  ?sensor ex:locatedIn ?zone .
  ?zone rdfs:label ?location_name .
}
```

This single query combines:
- Spatial filtering (PostGIS index scan)
- Property lookup (VP table join)
- Value filtering (temperature threshold)
- Graph traversal (zone→label)

Try expressing that in a system that separates the spatial index from the knowledge graph.

---

## Why PostGIS, Not a Custom Spatial Index

pg_ripple doesn't implement its own spatial indexing. PostGIS already exists, it's the best open-source spatial engine available, and it runs in the same PostgreSQL instance.

By translating GeoSPARQL to PostGIS, pg_ripple gets:
- GiST and SP-GiST indexes for spatial queries.
- Geography type with geodetic distance calculations.
- 300+ spatial functions beyond what GeoSPARQL defines.
- Decades of optimization for spatial operations.

The only requirement: install PostGIS alongside pg_ripple. Both are PostgreSQL extensions; they coexist without conflict.

```sql
CREATE EXTENSION postgis;
CREATE EXTENSION pg_ripple;
```

If PostGIS is not installed, GeoSPARQL functions return an error explaining the dependency. The rest of pg_ripple works fine — spatial queries are optional, not required.
