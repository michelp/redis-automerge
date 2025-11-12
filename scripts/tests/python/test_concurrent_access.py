"""
Concurrent access tests for Redis-Automerge.

Tests concurrent operations on the same document to verify CRDT behavior.
"""
import pytest
import asyncio


@pytest.mark.concurrent
async def test_concurrent_counter_increments(redis_client, clean_redis):
    """Test that concurrent counter increments are properly accumulated."""
    await redis_client.execute_command('AM.NEW', 'shared_counter')
    await redis_client.execute_command('AM.PUTCOUNTER', 'shared_counter', 'views', 0)

    # Run 5 concurrent increments
    await asyncio.gather(*[
        redis_client.execute_command('AM.INCCOUNTER', 'shared_counter', 'views', 1)
        for _ in range(5)
    ])

    # Counter should show sum of all increments
    value = await redis_client.execute_command('AM.GETCOUNTER', 'shared_counter', 'views')
    assert value == 5

    # Should have 6 changes total (1 putcounter + 5 increments)
    changes = await redis_client.execute_command('AM.NUMCHANGES', 'shared_counter')
    assert changes == 6


@pytest.mark.concurrent
async def test_concurrent_list_appends(redis_client, clean_redis):
    """Test that concurrent list appends all succeed."""
    await redis_client.execute_command('AM.NEW', 'shared_list')
    await redis_client.execute_command('AM.CREATELIST', 'shared_list', 'items')

    # Append 3 items concurrently
    items = ['item_1', 'item_2', 'item_3']
    await asyncio.gather(*[
        redis_client.execute_command('AM.APPENDTEXT', 'shared_list', 'items', item)
        for item in items
    ])

    # List should have all 3 items
    length = await redis_client.execute_command('AM.LISTLEN', 'shared_list', 'items')
    assert length == 3

    # Should have 4 changes (createlist + 3 appends)
    changes = await redis_client.execute_command('AM.NUMCHANGES', 'shared_list')
    assert changes == 4


@pytest.mark.concurrent
async def test_concurrent_nested_path_creation(redis_client, clean_redis):
    """Test that concurrent nested path operations all succeed."""
    await redis_client.execute_command('AM.NEW', 'shared_nested')

    # Create different nested paths concurrently
    await asyncio.gather(
        redis_client.execute_command('AM.PUTTEXT', 'shared_nested', 'user.profile.name', 'Alice'),
        redis_client.execute_command('AM.PUTINT', 'shared_nested', 'user.profile.age', 30),
        redis_client.execute_command('AM.PUTTEXT', 'shared_nested', 'user.settings.theme', 'dark'),
        redis_client.execute_command('AM.PUTBOOL', 'shared_nested', 'user.settings.notifications', 1)
    )

    # All fields should be accessible
    name = await redis_client.execute_command('AM.GETTEXT', 'shared_nested', 'user.profile.name')
    age = await redis_client.execute_command('AM.GETINT', 'shared_nested', 'user.profile.age')
    theme = await redis_client.execute_command('AM.GETTEXT', 'shared_nested', 'user.settings.theme')
    notif = await redis_client.execute_command('AM.GETBOOL', 'shared_nested', 'user.settings.notifications')

    assert name == b'Alice'
    assert age == 30
    assert theme == b'dark'
    assert notif == 1


@pytest.mark.concurrent
async def test_concurrent_edits_to_same_field(redis_client, clean_redis):
    """Test conflict resolution when multiple clients edit the same field."""
    await redis_client.execute_command('AM.NEW', 'shared_text')

    # Three concurrent edits to the same field (LWW - Last Write Wins)
    await asyncio.gather(
        redis_client.execute_command('AM.PUTTEXT', 'shared_text', 'content', 'version_1'),
        redis_client.execute_command('AM.PUTTEXT', 'shared_text', 'content', 'version_2'),
        redis_client.execute_command('AM.PUTTEXT', 'shared_text', 'content', 'version_3')
    )

    # One version will win (deterministic based on Automerge's algorithm)
    content = await redis_client.execute_command('AM.GETTEXT', 'shared_text', 'content')

    # Should be one of the versions
    assert content in [b'version_1', b'version_2', b'version_3']

    # Should have 3 changes recorded
    changes = await redis_client.execute_command('AM.NUMCHANGES', 'shared_text')
    assert changes == 3


@pytest.mark.concurrent
async def test_concurrent_operations_then_persistence(redis_client, clean_redis):
    """Test that concurrent operations persist correctly through save/load."""
    await redis_client.execute_command('AM.NEW', 'concurrent_doc')

    # Multiple concurrent operations of different types
    await asyncio.gather(
        redis_client.execute_command('AM.PUTTEXT', 'concurrent_doc', 'data.name', 'Test'),
        redis_client.execute_command('AM.PUTINT', 'concurrent_doc', 'data.count', 42),
        redis_client.execute_command('AM.PUTCOUNTER', 'concurrent_doc', 'data.views', 0)
    )

    await redis_client.execute_command('AM.INCCOUNTER', 'concurrent_doc', 'data.views', 10)

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'concurrent_doc')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'concurrent_doc')
    await redis_client.delete('concurrent_doc')
    await redis_client.execute_command('AM.LOAD', 'concurrent_doc', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'concurrent_doc')

    # Verify all data preserved
    name = await redis_client.execute_command('AM.GETTEXT', 'concurrent_doc', 'data.name')
    count = await redis_client.execute_command('AM.GETINT', 'concurrent_doc', 'data.count')
    views = await redis_client.execute_command('AM.GETCOUNTER', 'concurrent_doc', 'data.views')

    assert changes_before == changes_after == 4
    assert name == b'Test'
    assert count == 42
    assert views == 10


@pytest.mark.concurrent
async def test_concurrent_map_updates(redis_client, clean_redis):
    """Test that concurrent field creation results in correct map size."""
    await redis_client.execute_command('AM.NEW', 'concurrent_map')

    # Add 5 fields concurrently
    await asyncio.gather(*[
        redis_client.execute_command('AM.PUTTEXT', 'concurrent_map', f'field_{i}', f'value_{i}')
        for i in range(1, 6)
    ])

    # Should have 5 fields
    maplen = await redis_client.execute_command('AM.MAPLEN', 'concurrent_map', '')
    assert maplen == 5


@pytest.mark.concurrent
@pytest.mark.slow
async def test_stress_many_concurrent_increments(redis_client, clean_redis):
    """Stress test with many concurrent counter increments."""
    await redis_client.execute_command('AM.NEW', 'stress_counter')
    await redis_client.execute_command('AM.PUTCOUNTER', 'stress_counter', 'total', 0)

    # Launch 20 concurrent increments
    await asyncio.gather(*[
        redis_client.execute_command('AM.INCCOUNTER', 'stress_counter', 'total', 1)
        for _ in range(20)
    ])

    value = await redis_client.execute_command('AM.GETCOUNTER', 'stress_counter', 'total')
    assert value == 20


@pytest.mark.concurrent
async def test_concurrent_list_mixed_types(redis_client, clean_redis):
    """Test concurrent appends of different types to a list."""
    await redis_client.execute_command('AM.NEW', 'mixed_list')
    await redis_client.execute_command('AM.CREATELIST', 'mixed_list', 'mixed')

    # Append different types concurrently
    await asyncio.gather(
        redis_client.execute_command('AM.APPENDTEXT', 'mixed_list', 'mixed', 'text_value'),
        redis_client.execute_command('AM.APPENDINT', 'mixed_list', 'mixed', 123),
        redis_client.execute_command('AM.APPENDDOUBLE', 'mixed_list', 'mixed', 3.14),
        redis_client.execute_command('AM.APPENDBOOL', 'mixed_list', 'mixed', 1)
    )

    length = await redis_client.execute_command('AM.LISTLEN', 'mixed_list', 'mixed')
    assert length == 4


@pytest.mark.concurrent
async def test_interleaved_multi_field_updates(redis_client, clean_redis):
    """Test interleaved operations on multiple fields."""
    await redis_client.execute_command('AM.NEW', 'interleaved')

    # Run 3 rounds of concurrent updates to 2 fields
    for round_num in range(1, 4):
        await asyncio.gather(
            redis_client.execute_command('AM.PUTTEXT', 'interleaved', 'field_a', f'round_{round_num}_a'),
            redis_client.execute_command('AM.PUTTEXT', 'interleaved', 'field_b', f'round_{round_num}_b')
        )

    # Final values should be from round 3
    val_a = await redis_client.execute_command('AM.GETTEXT', 'interleaved', 'field_a')
    val_b = await redis_client.execute_command('AM.GETTEXT', 'interleaved', 'field_b')

    assert val_a == b'round_3_a'
    assert val_b == b'round_3_b'

    # Should have 6 changes (3 rounds × 2 fields)
    changes = await redis_client.execute_command('AM.NUMCHANGES', 'interleaved')
    assert changes == 6


@pytest.mark.concurrent
async def test_concurrent_counter_increment_decrement(redis_client, clean_redis):
    """Test concurrent increments and decrements on a counter."""
    await redis_client.execute_command('AM.NEW', 'balance')
    await redis_client.execute_command('AM.PUTCOUNTER', 'balance', 'amount', 100)

    # Concurrent increment and decrement
    await asyncio.gather(
        redis_client.execute_command('AM.INCCOUNTER', 'balance', 'amount', 50),
        redis_client.execute_command('AM.INCCOUNTER', 'balance', 'amount', -30)
    )

    # Should be 100 + 50 - 30 = 120
    value = await redis_client.execute_command('AM.GETCOUNTER', 'balance', 'amount')
    assert value == 120


@pytest.mark.concurrent
async def test_json_export_after_concurrent_ops(redis_client, clean_redis):
    """Test JSON export consistency after concurrent operations."""
    await redis_client.execute_command('AM.NEW', 'json_test')

    # Build complex structure concurrently
    await asyncio.gather(
        redis_client.execute_command('AM.PUTTEXT', 'json_test', 'user.name', 'Alice'),
        redis_client.execute_command('AM.PUTINT', 'json_test', 'user.age', 25),
        redis_client.execute_command('AM.PUTBOOL', 'json_test', 'user.active', 1)
    )

    # Export to JSON
    json_data = await redis_client.execute_command('AM.TOJSON', 'json_test')

    # JSON should contain all fields (basic validation)
    assert b'Alice' in json_data
    assert b'25' in json_data


@pytest.mark.concurrent
async def test_complex_concurrent_scenario(redis_client, clean_redis):
    """Test complex scenario with mixed concurrent operations."""
    await redis_client.execute_command('AM.NEW', 'complex_doc')

    # Initialize structures
    await redis_client.execute_command('AM.PUTCOUNTER', 'complex_doc', 'stats.views', 0)
    await redis_client.execute_command('AM.CREATELIST', 'complex_doc', 'tags')

    # Concurrent operations of different types
    await asyncio.gather(
        redis_client.execute_command('AM.INCCOUNTER', 'complex_doc', 'stats.views', 1),
        redis_client.execute_command('AM.APPENDTEXT', 'complex_doc', 'tags', 'tag1'),
        redis_client.execute_command('AM.PUTTEXT', 'complex_doc', 'metadata.author', 'Alice'),
        redis_client.execute_command('AM.INCCOUNTER', 'complex_doc', 'stats.views', 1),
        redis_client.execute_command('AM.APPENDTEXT', 'complex_doc', 'tags', 'tag2')
    )

    # Verify results
    views = await redis_client.execute_command('AM.GETCOUNTER', 'complex_doc', 'stats.views')
    tags_len = await redis_client.execute_command('AM.LISTLEN', 'complex_doc', 'tags')
    author = await redis_client.execute_command('AM.GETTEXT', 'complex_doc', 'metadata.author')

    assert views == 2
    assert tags_len == 2
    assert author == b'Alice'


@pytest.mark.concurrent
async def test_rapid_sequential_counter_operations(redis_client, clean_redis):
    """Test rapid sequential counter operations."""
    await redis_client.execute_command('AM.NEW', 'rapid_counter')
    await redis_client.execute_command('AM.PUTCOUNTER', 'rapid_counter', 'score', 0)

    # Rapidly increment and decrement (sequential but fast)
    for _ in range(10):
        await redis_client.execute_command('AM.INCCOUNTER', 'rapid_counter', 'score', 5)
        await redis_client.execute_command('AM.INCCOUNTER', 'rapid_counter', 'score', -2)

    # Should be 0 + (5-2)*10 = 30
    value = await redis_client.execute_command('AM.GETCOUNTER', 'rapid_counter', 'score')
    assert value == 30
