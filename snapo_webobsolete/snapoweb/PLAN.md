# Project Migration: CLI to Web App (Next.js + FastAPI)

## 1. Project Initialization

* [ ] **Backend Setup:**
* Create a `/backend` directory.
* Initialize a Python virtual environment (`python -m venv venv`).
* Install core dependencies: `fastapi`, `uvicorn`, `pydantic`.


* [ ] **Frontend Setup:**
* Create a `/frontend` directory using `npx create-next-app@latest`.
* Options: TypeScript, Tailwind CSS, and App Router enabled.
* Install `axios` or `tanstack/react-query` for data fetching.



## 2. Backend Strategy (FastAPI)

* [ ] **Core Logic Decoupling:** - Move existing terminal logic into a `services/` or `lib/` folder within `/backend`.
* Ensure functions accept parameters and return objects instead of using `input()` or `print()`.


* [ ] **API Implementation:**
* Create `main.py` with FastAPI.
* Define **Pydantic schemas** for request bodies and responses to ensure type safety.
* Implement `POST` endpoints for heavy actions (e.g., file processing, data analysis).
* Implement `GET` endpoints for status checks or data retrieval.


* [ ] **CORS Configuration:**
* Enable `CORSMiddleware` to allow requests from the Next.js dev server (`localhost:3000`).



## 3. Frontend Strategy (Next.js)

* [ ] **Layout Design:**
* Build a "Dashboard" style layout with a sidebar and main content area.
* Use **Lucide-React** for iconography to give it a desktop-app feel.


* [ ] **State Management:**
* Implement a loading state for long-running Python tasks.
* Use `useEffect` or `React Query` to poll the backend if a task is asynchronous.


* [ ] **Component Mapping:**
* Map CLI arguments to UI components:
* `--input-file` $\rightarrow$ File Upload component.
* `--toggle-option` $\rightarrow$ Switch/Checkbox.
* `--category` $\rightarrow$ Select/Dropdown.





## 4. Development Workflow

* [ ] **Concurrent Running:**
* Add a root `package.json` with a `dev` script using `concurrently` to start both the Uvicorn server and Next.js dev server with one command.


* [ ] **Environment Variables:**
* Set `NEXT_PUBLIC_API_URL=http://localhost:8000` for frontend calls.



## 5. Definition of Done (Phase 1)

* [ ] The Python logic can be triggered via a web form.
* [ ] Results from the Python script are displayed clearly in the browser.
* [ ] Errors in Python are caught and displayed as UI toast notifications.

---

