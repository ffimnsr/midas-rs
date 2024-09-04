default: run-pgsql

run-pgsql:
	docker run -it --rm --name pgsql -p 5432:5432 \
		-e POSTGRES_PASSWORD=postgres \
		-e POSTGRES_DB=startup \
		-d postgres:16-alpine

run-mysql:
	docker run -it --rm --name mysql -p 3306:3306 \
		-e MYSQL_ROOT_PASSWORD=mysql \
		-e MYSQL_DATABASE=startup \
		-d mysql:9-oracle

run-mariadb:
	docker run -it --rm --name mariadb -p 3306:3306 \
		-e MARIADB_ROOT_PASSWORD=mariadb \
		-e MARIADB_DATABASE=startup \
		-d mariadb:10

run-mssql:
	docker run -it --rm --name mssql -p 1433:1433 \
		-e ACCEPT_EULA=Y \
		-e MSSQL_SA_PASSWORD='Strong(!)Password123' \
		-e MSSQL_PID=Developer \
		-e MSSQL_DB=startup \
		-d mcr.microsoft.com/mssql/server:2022-latest

format:
	cargo fmt --all

clippy:
	cargo clippy --all-features --all-targets --tests --benches -- -Dclippy::all

.PHONY: run-pgsql run-mysql run-mariadb run-mssql
