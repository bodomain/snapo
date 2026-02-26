# The Architecture of Prodz: A Full-Stack Productivity Application

## Chapter 1: Introduction and System Overview

Modern web applications require a separation of concerns that cleanly divides user interface logic, business rules, and data persistence. The **Prodz** productivity tracker exemplifies this paradigm by implementing a robust multi-tier architecture. Designed to assist users in managing focused work sessions and analyzing their productivity, the application utilizes a modern technology stack deployed within isolated, communicative containers.

The application is structured into four primary technical domains:
1. **The Presentation Layer (Frontend)**: Implemented using Next.js and React, providing an interactive interface deployed as a standalone Node.js server.
2. **The Application Layer (Backend)**: Developed with Python and the FastAPI framework, handling business logic, API routing, and data processing.
3. **The Data Layer (Database)**: Utilizing SQLite for lightweight, reliable, high-performance data persistence mounted directly into the application layer.
4. **The Gateway Layer (Reverse Proxy)**: Powered by Nginx, routing traffic, terminating SSL, and serving as the primary entry point for external web requests.

All layers are encapsulated using Docker, ensuring that the application remains environment-agnostic, scalable, and reproducible across development and production environments.

---

## Chapter 2: The Presentation Layer

The frontend of the Prodz application serves as the user's primary interaction point. It is built upon **Next.js**, a powerful React framework that enables both server-side rendering (SSR) and static site generation (SSG). 

### 2.1 Technology Stack
The presentation layer leverages the following core technologies:
* **Framework**: Next.js 15+ (React 19)
* **Language**: TypeScript, ensuring type safety and reducing runtime errors during development.
* **Styling**: Tailwind CSS, utilizing a utility-first approach to craft responsive, aesthetically pleasing interfaces without the overhead of traditional CSS stylesheets.
* **Visualization**: Recharts, a composable charting library built on React components, used for rendering statistical data concerning user productivity.

### 2.2 Core Components
The interface is structurally divided into declarative, reusable components:
* **TimerComponent**: Manages the state of the Pomodoro timer, handling interval calculations and orchestrating session completion events.
* **StatsComponent**: Retrieves, aggregates, and visualizes historical session data.
* **MergeDBComponent**: Provides a drag-and-drop or file selection interface allowing users to upload external SQLite database files for merging.

### 2.3 Deployment State
The frontend is compiled using the `output: 'standalone'` directive. This generates a heavily optimized, self-contained Next.js server that includes only the necessary files for production execution. This standalone server operates on port 3000 within its Docker container.

---

## Chapter 3: The Application Layer

The backend tier acts as the central nervous system of Prodz, orchestrating data flow between the user interface and the underlying database. It prioritizes speed, developer ergonomics, and concurrency.

### 3.1 Technology Stack
The backend is constructed using:
* **Framework**: FastAPI (Python 3.11+)
* **Server**: Uvicorn, an ASGI (Asynchronous Server Gateway Interface) web server implementation for Python.
* **Validation**: Pydantic, which enforces strict type hints and data validation at the API boundary, guaranteeing that incoming payloads conform to expected schemas.

### 3.2 API Interface
The backend exposes a RESTful interface over standard HTTP, mapping routes to specific business functions:
* `POST /api/sessions/log`: Accepts JSON payloads containing session details (activity name and duration) and delegates the insertion to the database layer.
* `GET /api/stats`: Queries the database for historical activity data and serializes the result into JSON formats consumable by the frontend visualization components.
* `POST /api/database/merge`: Parses multipart/form-data containing `.db` files, temporarily staging the file to execute a SQLite schema merge operation.
* `GET /health`: A lightweight diagnostic endpoint utilized by Docker to verify container stability and readiness.

### 3.3 Concurrency and Asynchronicity
FastAPI naturally supports asynchronous execution via Python's `asyncio` library. Endpoints heavily reliant on I/O operations (such as file uploads during the database merge process) utilize `async def` definitions to prevent thread-blocking, ensuring high throughput under concurrent load.

---

## Chapter 4: Data Management

Persistent data storage within Prodz diverges from heavy relational database management systems (RDBMS) in favor of a localized, high-performance embedded database.

### 4.1 The SQLite Paradigm
The application employs **SQLite** (`prodz.db`). Unlike client-server SQL engines (e.g., PostgreSQL or MySQL), SQLite stores the entire application state in a single, cross-platform file on the host filesystem. 

This approach was selected for the following architectural benefits:
1. **Zero Configuration**: Eliminates the need for dedicated database container provisioning, intricate user permissions (GRANT/REVOKE), and network configurations.
2. **Portability**: Users can easily back up their productivity data by duplicating the `prodz.db` file, or migrate data across devices using the UI's database merge functionality.
3. **Reduced Overhead**: Considerably lowers the memory footprint of the Docker deployment.

### 4.2 Schema Design
The primary schema consists of a `sessions` table defined by the following structure:
```sql
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    date TEXT NOT NULL,
    timestamp TEXT NOT NULL,
    activity TEXT NOT NULL,
    duration_minutes REAL NOT NULL
);
```

### 4.3 Database Integration
The database file is mounted directly into the backend container at `/app/prodz.db` via Docker volumes. The backend Python application interacts with this file utilizing the standard `sqlite3` driver. When a user uploads an external database for merging, the backend temporarily attaches the external database to the active instance (`ATTACH DATABASE ? AS foreign_db`) and executes a conditional `INSERT ... SELECT` statement to deduplicate and ingest new records.

---

## Chapter 5: Network Gateway and Reverse Proxy

To efficiently and securely route incoming traffic to the appropriate internal services, Prodz utilizes an internet-facing gateway layer.

### 5.1 The Nginx Proxy
**Nginx**, operating in a lightweight Alpine Linux container, acts as the reverse proxy. It listens on standard web ports (80 for HTTP and 443 for HTTPS). 

### 5.2 Routing Rules
The proxy utilizes path-based routing logic defined in `nginx.conf`:
* **API Traffic**: Requests matching the `/api/*` URI are forwarded to the internal `backend` service continuously listening on port 8000. It also enforces request rate limiting (`limit_req_zone`) to mitigate denial-of-service vectors.
* **Frontend Traffic**: Requests mapped to the root `/` URI are forwarded to the Next.js `frontend` service listening on port 3000. It utilizes HTTP/1.1 and manages proxy headers to support potential WebSocket connections commonly used by React frameworks for hot-module reloading or dynamic data streaming.

### 5.3 Transport Layer Security
Nginx serves as the SSL/TLS termination point. By managing cryptographic certificates (e.g., `cert.pem` and `key.pem`) at the network boundary, the internal services (Next.js and FastAPI) are relieved from the computational overhead of decrypting traffic, streamlining internal communication over standard HTTP.

---

## Chapter 6: Containerization and Orchestration

The application utilizes **Docker Compose** to define, build, and run the multi-container architecture.

### 6.1 Multi-stage Docker Builds
Both the frontend and backend utilize multi-stage Dockerfiles. This software engineering paradigm separates the *build environment* from the *runtime environment*.
* **Backend**: Dependencies are compiled and packages are fetched using `pip` within a bloated builder image. Only the resultant `/usr/local/lib/python3.11/site-packages` directory is copied into the final, highly-compact production image.
* **Frontend**: The Node.js application is built, transformed, and pruned by `npm` in a build stage. The final `runner` stage acquires only the `.next/standalone` optimized executable code.

### 6.2 Service Dependencies and Network Isolation
Using `.yml` definitions, the architecture is strictly ordered using `depends_on` clauses. The gateway proxy will not accept traffic until both the frontend and backend report functional `healthy` states. 
All containers communicate privately over an isolated bridge network (`snapo_network`). Nginx is the only service that publishes ports to the host machine, ensuring that the backend application server and Node environment are strictly shielded from public internet access.