"""
pytest configuration and fixtures for Redis-Automerge tests.
"""
import os
import pytest
import pytest_asyncio
from redis.asyncio import Redis


@pytest_asyncio.fixture
async def redis_client():
    """
    Async Redis client fixture with binary data handling.

    Yields a Redis client connected to the test instance.
    Automatically closes connection after test completion.
    """
    host = os.getenv('REDIS_HOST', 'localhost')
    port = int(os.getenv('REDIS_PORT', 6379))

    # Create client with decode_responses=False for binary data
    client = Redis(
        host=host,
        port=port,
        decode_responses=False,  # Return bytes, not strings
        socket_timeout=5.0,
        socket_connect_timeout=5.0
    )

    # Verify connection
    await client.ping()

    yield client

    # Cleanup
    await client.aclose()


@pytest_asyncio.fixture
async def clean_redis(redis_client):
    """
    Flush the database before each test.

    Ensures tests start with a clean slate.
    """
    await redis_client.flushdb()
    yield
    # Optionally flush after test as well
    await redis_client.flushdb()


@pytest_asyncio.fixture
async def sample_document(redis_client, clean_redis):
    """
    Create a sample document with various data types.

    Useful for tests that need a pre-populated document.
    """
    key = 'sample_doc'

    await redis_client.execute_command('AM.NEW', key)
    await redis_client.execute_command('AM.PUTTEXT', key, 'name', 'Alice')
    await redis_client.execute_command('AM.PUTINT', key, 'age', 30)
    await redis_client.execute_command('AM.PUTDOUBLE', key, 'score', 95.5)
    await redis_client.execute_command('AM.PUTBOOL', key, 'active', 1)
    await redis_client.execute_command('AM.PUTCOUNTER', key, 'views', 0)
    await redis_client.execute_command('AM.INCCOUNTER', key, 'views', 10)

    return key


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "slow: marks tests as slow"
    )
    config.addinivalue_line(
        "markers", "concurrent: marks tests that test concurrent operations"
    )
    config.addinivalue_line(
        "markers", "sync: marks tests that test document synchronization"
    )
    config.addinivalue_line(
        "markers", "persistence: marks tests that test save/load functionality"
    )
