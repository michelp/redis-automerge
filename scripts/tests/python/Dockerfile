FROM python:3.11-slim

WORKDIR /tests

# Install dependencies
COPY requirements.txt .
RUN pip install --no-cache-dir -r requirements.txt

# Copy test files
COPY . .

# Default command: run all tests
CMD ["pytest", "-v", "--tb=short", "--color=yes"]
