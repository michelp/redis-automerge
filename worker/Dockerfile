FROM python:3.12-alpine

WORKDIR /app

# Install dependencies for psycopg2
RUN apk add --no-cache postgresql-dev gcc musl-dev

# Copy requirements and install Python dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy worker script
COPY archive_worker.py .

# Make script executable
RUN chmod +x archive_worker.py

# Run the worker
CMD ["python", "-u", "archive_worker.py"]
