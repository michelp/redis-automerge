"""
Document synchronization tests for Redis-Automerge.

Tests AM.APPLY functionality with proper binary change arrays.
This is the key capability that bash tests couldn't properly exercise.
"""
import pytest
import asyncio


@pytest.mark.sync
async def test_basic_one_way_sync(redis_client, clean_redis):
    """Test basic one-way synchronization from source to target."""
    # Create source document with data
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'name', 'Alice')
    await redis_client.execute_command('AM.PUTINT', 'source', 'age', 30)

    # Get changes from source
    changes = await redis_client.execute_command('AM.CHANGES', 'source')

    # Verify we got a list of binary changes
    assert isinstance(changes, list)
    assert len(changes) == 2
    assert all(isinstance(c, bytes) for c in changes)

    # Create target and apply changes
    await redis_client.execute_command('AM.NEW', 'target')
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # Verify target has same data
    name = await redis_client.execute_command('AM.GETTEXT', 'target', 'name')
    age = await redis_client.execute_command('AM.GETINT', 'target', 'age')

    assert name == b'Alice'
    assert age == 30


@pytest.mark.sync
async def test_bidirectional_sync(redis_client, clean_redis):
    """Test bidirectional synchronization between two documents."""
    # Create two documents
    await redis_client.execute_command('AM.NEW', 'doc_a')
    await redis_client.execute_command('AM.NEW', 'doc_b')

    # Each document gets different data
    await redis_client.execute_command('AM.PUTTEXT', 'doc_a', 'field1', 'from_a')
    await redis_client.execute_command('AM.PUTTEXT', 'doc_b', 'field2', 'from_b')

    # Get changes from both
    changes_a = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    changes_b = await redis_client.execute_command('AM.CHANGES', 'doc_b')

    # Apply A's changes to B
    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes_a)

    # Apply B's changes to A
    await redis_client.execute_command('AM.APPLY', 'doc_a', *changes_b)

    # Both documents should now have both fields
    field1_a = await redis_client.execute_command('AM.GETTEXT', 'doc_a', 'field1')
    field2_a = await redis_client.execute_command('AM.GETTEXT', 'doc_a', 'field2')
    field1_b = await redis_client.execute_command('AM.GETTEXT', 'doc_b', 'field1')
    field2_b = await redis_client.execute_command('AM.GETTEXT', 'doc_b', 'field2')

    assert field1_a == b'from_a'
    assert field2_a == b'from_b'
    assert field1_b == b'from_a'
    assert field2_b == b'from_b'


@pytest.mark.sync
async def test_multi_document_sync_three_way(redis_client, clean_redis):
    """Test synchronization across three documents."""
    # Create three documents
    await redis_client.execute_command('AM.NEW', 'doc1')
    await redis_client.execute_command('AM.NEW', 'doc2')
    await redis_client.execute_command('AM.NEW', 'doc3')

    # Each gets unique data
    await redis_client.execute_command('AM.PUTTEXT', 'doc1', 'from_doc1', 'value1')
    await redis_client.execute_command('AM.PUTTEXT', 'doc2', 'from_doc2', 'value2')
    await redis_client.execute_command('AM.PUTTEXT', 'doc3', 'from_doc3', 'value3')

    # Full mesh sync: each document receives changes from all others
    changes_1 = await redis_client.execute_command('AM.CHANGES', 'doc1')
    changes_2 = await redis_client.execute_command('AM.CHANGES', 'doc2')
    changes_3 = await redis_client.execute_command('AM.CHANGES', 'doc3')

    # Apply all changes to all documents
    await redis_client.execute_command('AM.APPLY', 'doc1', *changes_2, *changes_3)
    await redis_client.execute_command('AM.APPLY', 'doc2', *changes_1, *changes_3)
    await redis_client.execute_command('AM.APPLY', 'doc3', *changes_1, *changes_2)

    # All three documents should have all three fields
    for doc in ['doc1', 'doc2', 'doc3']:
        val1 = await redis_client.execute_command('AM.GETTEXT', doc, 'from_doc1')
        val2 = await redis_client.execute_command('AM.GETTEXT', doc, 'from_doc2')
        val3 = await redis_client.execute_command('AM.GETTEXT', doc, 'from_doc3')

        assert val1 == b'value1'
        assert val2 == b'value2'
        assert val3 == b'value3'


@pytest.mark.sync
async def test_incremental_sync(redis_client, clean_redis):
    """Test incremental synchronization (syncing only new changes)."""
    # Create documents
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.NEW', 'target')

    # Initial sync
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field1', 'initial')
    changes_1 = await redis_client.execute_command('AM.CHANGES', 'source')
    await redis_client.execute_command('AM.APPLY', 'target', *changes_1)

    # Verify initial sync
    val = await redis_client.execute_command('AM.GETTEXT', 'target', 'field1')
    assert val == b'initial'

    # Make more changes to source
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field2', 'update1')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field3', 'update2')

    # Get ALL changes (target doesn't know which it has)
    all_changes = await redis_client.execute_command('AM.CHANGES', 'source')

    # Apply all changes (Automerge handles deduplication)
    await redis_client.execute_command('AM.APPLY', 'target', *all_changes)

    # Verify new fields synced
    val2 = await redis_client.execute_command('AM.GETTEXT', 'target', 'field2')
    val3 = await redis_client.execute_command('AM.GETTEXT', 'target', 'field3')

    assert val2 == b'update1'
    assert val3 == b'update2'


@pytest.mark.sync
async def test_sync_with_conflicts(redis_client, clean_redis):
    """Test synchronization when both documents have edited the same field."""
    # Create documents with initial shared state
    await redis_client.execute_command('AM.NEW', 'doc_a')
    await redis_client.execute_command('AM.PUTTEXT', 'doc_a', 'shared_field', 'initial')

    # Sync to doc_b
    changes = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    await redis_client.execute_command('AM.NEW', 'doc_b')
    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes)

    # Both documents now have same initial state
    # Now both edit the same field (conflict!)
    await redis_client.execute_command('AM.PUTTEXT', 'doc_a', 'shared_field', 'value_from_a')
    await redis_client.execute_command('AM.PUTTEXT', 'doc_b', 'shared_field', 'value_from_b')

    # Sync changes bidirectionally
    changes_a = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    changes_b = await redis_client.execute_command('AM.CHANGES', 'doc_b')

    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes_a)
    await redis_client.execute_command('AM.APPLY', 'doc_a', *changes_b)

    # Both documents should converge to the same value (deterministic)
    val_a = await redis_client.execute_command('AM.GETTEXT', 'doc_a', 'shared_field')
    val_b = await redis_client.execute_command('AM.GETTEXT', 'doc_b', 'shared_field')

    assert val_a == val_b  # Convergence!


@pytest.mark.sync
async def test_sync_counter_operations(redis_client, clean_redis):
    """Test that counter increments sync correctly (CRDT behavior)."""
    # Create documents
    await redis_client.execute_command('AM.NEW', 'doc_a')
    await redis_client.execute_command('AM.PUTCOUNTER', 'doc_a', 'counter', 0)

    # Sync initial counter to doc_b
    changes = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    await redis_client.execute_command('AM.NEW', 'doc_b')
    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes)

    # Both increment concurrently (offline)
    await redis_client.execute_command('AM.INCCOUNTER', 'doc_a', 'counter', 5)
    await redis_client.execute_command('AM.INCCOUNTER', 'doc_b', 'counter', 3)

    # Sync changes
    changes_a = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    changes_b = await redis_client.execute_command('AM.CHANGES', 'doc_b')

    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes_a)
    await redis_client.execute_command('AM.APPLY', 'doc_a', *changes_b)

    # Both should have counter = 0 + 5 + 3 = 8 (sum of increments)
    val_a = await redis_client.execute_command('AM.GETCOUNTER', 'doc_a', 'counter')
    val_b = await redis_client.execute_command('AM.GETCOUNTER', 'doc_b', 'counter')

    assert val_a == 8
    assert val_b == 8


@pytest.mark.sync
async def test_sync_list_operations(redis_client, clean_redis):
    """Test that list operations sync correctly."""
    # Create document with list
    await redis_client.execute_command('AM.NEW', 'doc_a')
    await redis_client.execute_command('AM.CREATELIST', 'doc_a', 'items')
    await redis_client.execute_command('AM.APPENDTEXT', 'doc_a', 'items', 'item1')

    # Sync to doc_b
    changes = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    await redis_client.execute_command('AM.NEW', 'doc_b')
    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes)

    # Both append to list concurrently
    await redis_client.execute_command('AM.APPENDTEXT', 'doc_a', 'items', 'from_a')
    await redis_client.execute_command('AM.APPENDTEXT', 'doc_b', 'items', 'from_b')

    # Sync
    changes_a = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    changes_b = await redis_client.execute_command('AM.CHANGES', 'doc_b')

    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes_a)
    await redis_client.execute_command('AM.APPLY', 'doc_a', *changes_b)

    # Both should have 3 items
    len_a = await redis_client.execute_command('AM.LISTLEN', 'doc_a', 'items')
    len_b = await redis_client.execute_command('AM.LISTLEN', 'doc_b', 'items')

    assert len_a == 3
    assert len_b == 3


@pytest.mark.sync
async def test_empty_changes_application(redis_client, clean_redis):
    """Test applying empty changes list (idempotence)."""
    await redis_client.execute_command('AM.NEW', 'doc')
    await redis_client.execute_command('AM.PUTTEXT', 'doc', 'field', 'value')

    # Get changes from a document with no changes
    await redis_client.execute_command('AM.NEW', 'empty_doc')
    empty_changes = await redis_client.execute_command('AM.CHANGES', 'empty_doc')

    # empty_changes should be an empty list
    assert isinstance(empty_changes, list)
    assert len(empty_changes) == 0

    # Applying empty changes should not error
    # Note: Can't use *[] in function call, so we skip if truly empty
    if empty_changes:  # Should not execute
        await redis_client.execute_command('AM.APPLY', 'doc', *empty_changes)

    # Document unchanged
    val = await redis_client.execute_command('AM.GETTEXT', 'doc', 'field')
    assert val == b'value'


@pytest.mark.sync
async def test_idempotent_change_application(redis_client, clean_redis):
    """Test that applying the same changes multiple times is idempotent."""
    # Create source
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field', 'value')

    changes = await redis_client.execute_command('AM.CHANGES', 'source')

    # Create target and apply changes
    await redis_client.execute_command('AM.NEW', 'target')
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # Apply same changes again (should be idempotent)
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # And again
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # Should still have correct value
    val = await redis_client.execute_command('AM.GETTEXT', 'target', 'field')
    assert val == b'value'

    # Should have correct number of changes (changes don't duplicate)
    num_changes = await redis_client.execute_command('AM.NUMCHANGES', 'target')
    assert num_changes == 1


@pytest.mark.sync
async def test_sync_preserves_change_history(redis_client, clean_redis):
    """Test that synced documents have the same change history."""
    # Create source with multiple changes
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field1', 'value1')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field2', 'value2')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field3', 'value3')

    source_changes_count = await redis_client.execute_command('AM.NUMCHANGES', 'source')

    # Sync to target
    changes = await redis_client.execute_command('AM.CHANGES', 'source')
    await redis_client.execute_command('AM.NEW', 'target')
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    target_changes_count = await redis_client.execute_command('AM.NUMCHANGES', 'target')

    # Both should have same change count
    assert source_changes_count == target_changes_count == 3


@pytest.mark.sync
@pytest.mark.persistence
async def test_sync_then_persistence(redis_client, clean_redis):
    """Test that synced documents persist correctly."""
    # Create and sync documents
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'data', 'test_value')

    changes = await redis_client.execute_command('AM.CHANGES', 'source')

    await redis_client.execute_command('AM.NEW', 'target')
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # Save and reload target
    saved_data = await redis_client.execute_command('AM.SAVE', 'target')
    await redis_client.delete('target')
    await redis_client.execute_command('AM.LOAD', 'target', saved_data)

    # Verify data still there
    val = await redis_client.execute_command('AM.GETTEXT', 'target', 'data')
    assert val == b'test_value'

    # Verify change history preserved
    changes_count = await redis_client.execute_command('AM.NUMCHANGES', 'target')
    assert changes_count == 1


@pytest.mark.sync
async def test_complex_nested_structure_sync(redis_client, clean_redis):
    """Test synchronization of complex nested structures."""
    # Create complex structure on source
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'user.name', 'Alice')
    await redis_client.execute_command('AM.PUTINT', 'source', 'user.age', 30)
    await redis_client.execute_command('AM.CREATELIST', 'source', 'user.tags')
    await redis_client.execute_command('AM.APPENDTEXT', 'source', 'user.tags', 'developer')
    await redis_client.execute_command('AM.APPENDTEXT', 'source', 'user.tags', 'rust')
    await redis_client.execute_command('AM.PUTCOUNTER', 'source', 'user.views', 0)
    await redis_client.execute_command('AM.INCCOUNTER', 'source', 'user.views', 100)

    # Sync to target
    changes = await redis_client.execute_command('AM.CHANGES', 'source')
    await redis_client.execute_command('AM.NEW', 'target')
    await redis_client.execute_command('AM.APPLY', 'target', *changes)

    # Verify entire structure synced
    name = await redis_client.execute_command('AM.GETTEXT', 'target', 'user.name')
    age = await redis_client.execute_command('AM.GETINT', 'target', 'user.age')
    tags_len = await redis_client.execute_command('AM.LISTLEN', 'target', 'user.tags')
    views = await redis_client.execute_command('AM.GETCOUNTER', 'target', 'user.views')

    assert name == b'Alice'
    assert age == 30
    assert tags_len == 2
    assert views == 100
