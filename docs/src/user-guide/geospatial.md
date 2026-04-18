# Geospatial Data

pg_ripple supports geographic data through its [GeoSPARQL 1.1 subset](../reference/geosparql.md), which delegates geometry operations to [PostGIS](https://postgis.net).

## Storing Geographic Data

Store geometries as WKT (Well-Known Text) literals in N-Triples or Turtle format:

```sql
-- Load a city as a point
SELECT pg_ripple.load_ntriples(
    '<https://data.example/city/berlin> '
    '<https://www.w3.org/ns/locn#geometry> '
    '"POINT(13.404954 52.520008)" .'
);

-- Load a region as a polygon
SELECT pg_ripple.load_ntriples(
    '<https://data.example/region/central-europe> '
    '<https://www.w3.org/ns/locn#geometry> '
    '"POLYGON((10.0 47.0, 24.0 47.0, 24.0 55.0, 10.0 55.0, 10.0 47.0))" .'
);
```

Or use Turtle with inline literals:

```turtle
@prefix locn: <https://www.w3.org/ns/locn#> .

<https://data.example/city/berlin>
    locn:geometry "POINT(13.404954 52.520008)" .
```

## Querying Geographic Data

### Filtering by intersection

Find all cities within a bounding box:

```sparql
PREFIX geo: <http://www.opengis.net/def/function/geosparql/>

SELECT ?city ?geom WHERE {
  ?city <https://www.w3.org/ns/locn#geometry> ?geom .
  FILTER(geo:sfIntersects(?geom,
    "POLYGON((10.0 47.0, 24.0 47.0, 24.0 55.0, 10.0 55.0, 10.0 47.0))"))
}
```

### Filtering by containment

Find all points within a polygon:

```sparql
PREFIX geo: <http://www.opengis.net/def/function/geosparql/>

SELECT ?place WHERE {
  ?place <https://www.w3.org/ns/locn#geometry> ?geom .
  FILTER(geo:sfWithin(?geom,
    "POLYGON((12.0 51.0, 15.0 51.0, 15.0 54.0, 12.0 54.0, 12.0 51.0))"))
}
```

### Computing distances

Find all places within 50 km of Berlin:

```sparql
PREFIX geof: <http://www.opengis.net/def/function/geosparql/>

SELECT ?place ?dist WHERE {
  ?place <https://www.w3.org/ns/locn#geometry> ?geom .
  BIND(geof:distance(?geom, "POINT(13.404954 52.520008)",
       <http://www.opengis.net/def/uom/OGC/1.0/metre>) AS ?dist)
  FILTER(?dist < 50000)
}
ORDER BY ?dist
```

## Checking PostGIS Availability

```sql
SELECT EXISTS(
  SELECT 1 FROM pg_proc WHERE proname = 'st_geomfromtext'
) AS postgis_available;
```

If PostGIS is not available, all geo FILTER functions return `false` (queries return zero rows) and all geo BIND functions return `NULL` — no errors are raised.

## Installing PostGIS

If PostGIS is not yet installed:

```sql
CREATE EXTENSION postgis;
```

Or on most Linux distributions:

```bash
# Debian/Ubuntu
sudo apt-get install postgresql-18-postgis-3

# RHEL/CentOS
sudo dnf install postgresql18-postgis33
```

## Supported Geometry Types

All geometry types supported by PostGIS can be stored as WKT literals:

| Type | Example WKT |
|---|---|
| Point | `POINT(13.4 52.5)` |
| LineString | `LINESTRING(0 0, 10 10, 20 5)` |
| Polygon | `POLYGON((0 0, 10 0, 10 10, 0 10, 0 0))` |
| MultiPoint | `MULTIPOINT((0 0), (10 10))` |
| MultiPolygon | `MULTIPOLYGON(((0 0, 4 0, 4 4, 0 4, 0 0)), ((5 5, 9 5, 9 9, 5 9, 5 5)))` |

## Performance Tips

- Index the geometry predicate VP table using PostGIS spatial index after loading:
  ```sql
  CREATE INDEX ON _pg_ripple.vp_{predicate_id}_delta
    USING GIST (ST_GeomFromText(
      (SELECT value FROM _pg_ripple.dictionary WHERE id = o)
    ));
  ```
- Load large geography datasets with `pg_ripple.load_ntriples_file()` rather than inline strings for better memory efficiency.
- Use `geo:sfDisjoint` sparingly — it requires scanning all geometry pairs.
