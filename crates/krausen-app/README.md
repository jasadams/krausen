# Application foundation tests

The integration suite requires a disposable PostgreSQL database and runs the
same behavior against SQLite and PostgreSQL. It applies server migrations; do
not point it at a production or shared development database. A local container
is one way to provide an isolated database:

```sh
podman run -d --rm --name krausen-test-db \
  -e POSTGRES_USER=krausen -e POSTGRES_PASSWORD=krausen \
  -e POSTGRES_DB=krausen_test -p 127.0.0.1::5432 docker.io/library/postgres:16
podman port krausen-test-db 5432
# Use the reported loopback port in this URL, then run:
KRAUSEN_TEST_POSTGRES_URL=postgres://krausen:krausen@127.0.0.1:PORT/krausen_test \
  cargo test -p krausen-app --test foundation
podman stop krausen-test-db
```

Wait for the database to become ready before running the suite. The PostgreSQL
test fails if its URL is absent; it never silently skips. CI starts a fresh
PostgreSQL service and runs both backend tests. Each developer should use a
separate disposable database/container.
