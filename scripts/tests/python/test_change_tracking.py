"""
Change tracking and persistence tests for Redis-Automerge.

Tests that changes are properly tracked and preserved through save/load cycles.
"""
import pytest


@pytest.mark.persistence
async def test_single_change_preservation(redis_client, clean_redis):
    """Test that a single change is preserved through save/load."""
    # Create document with one change
    await redis_client.execute_command('AM.NEW', 'test1')
    await redis_client.execute_command('AM.PUTTEXT', 'test1', 'field1', 'value1')

    # Get change count before save
    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test1')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test1')
    await redis_client.delete('test1')
    await redis_client.execute_command('AM.LOAD', 'test1', saved_data)

    # Verify change count preserved
    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test1')
    assert changes_before == changes_after == 1

    # Verify data preserved
    value = await redis_client.execute_command('AM.GETTEXT', 'test1', 'field1')
    assert value == b'value1'


@pytest.mark.persistence
async def test_multiple_changes_preservation(redis_client, clean_redis):
    """Test that multiple changes are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test2')

    # Make 5 different changes
    await redis_client.execute_command('AM.PUTTEXT', 'test2', 'field1', 'value1')
    await redis_client.execute_command('AM.PUTINT', 'test2', 'field2', 42)
    await redis_client.execute_command('AM.PUTDOUBLE', 'test2', 'field3', 3.14)
    await redis_client.execute_command('AM.PUTBOOL', 'test2', 'field4', 1)
    await redis_client.execute_command('AM.PUTTEXT', 'test2', 'field5', 'value5')

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test2')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test2')
    await redis_client.delete('test2')
    await redis_client.execute_command('AM.LOAD', 'test2', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test2')
    assert changes_before == changes_after == 5

    # Verify all values preserved
    assert await redis_client.execute_command('AM.GETTEXT', 'test2', 'field1') == b'value1'
    assert await redis_client.execute_command('AM.GETINT', 'test2', 'field2') == 42
    assert await redis_client.execute_command('AM.GETDOUBLE', 'test2', 'field3') == b'3.14'
    assert await redis_client.execute_command('AM.GETBOOL', 'test2', 'field4') == 1


@pytest.mark.persistence
@pytest.mark.slow
async def test_large_change_history_50(redis_client, clean_redis):
    """Test that a large change history (50 changes) is preserved."""
    await redis_client.execute_command('AM.NEW', 'test3')

    # Make 50 changes to the same field
    for i in range(1, 51):
        await redis_client.execute_command('AM.PUTINT', 'test3', 'counter', i)

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test3')
    assert changes_before == 50

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test3')
    await redis_client.delete('test3')
    await redis_client.execute_command('AM.LOAD', 'test3', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test3')
    assert changes_after == 50

    # Verify final value
    final_value = await redis_client.execute_command('AM.GETINT', 'test3', 'counter')
    assert final_value == 50


@pytest.mark.persistence
@pytest.mark.slow
async def test_very_large_change_history_200(redis_client, clean_redis):
    """Test that a very large change history (200 changes) is preserved."""
    await redis_client.execute_command('AM.NEW', 'test11')

    # Make 200 changes
    for i in range(1, 201):
        await redis_client.execute_command('AM.PUTINT', 'test11', 'counter', i)

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test11')
    assert changes_before == 200

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test11')
    await redis_client.delete('test11')
    await redis_client.execute_command('AM.LOAD', 'test11', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test11')
    assert changes_after == 200

    final_value = await redis_client.execute_command('AM.GETINT', 'test11', 'counter')
    assert final_value == 200


@pytest.mark.persistence
async def test_changes_before_and_after_save_cycle(redis_client, clean_redis):
    """Test that changes made before and after save/load are both tracked."""
    await redis_client.execute_command('AM.NEW', 'test4')

    # Make 3 changes before save
    await redis_client.execute_command('AM.PUTTEXT', 'test4', 'field1', 'before1')
    await redis_client.execute_command('AM.PUTTEXT', 'test4', 'field2', 'before2')
    await redis_client.execute_command('AM.PUTTEXT', 'test4', 'field3', 'before3')

    changes_before_save = await redis_client.execute_command('AM.NUMCHANGES', 'test4')
    assert changes_before_save == 3

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test4')
    await redis_client.delete('test4')
    await redis_client.execute_command('AM.LOAD', 'test4', saved_data)

    changes_after_load = await redis_client.execute_command('AM.NUMCHANGES', 'test4')
    assert changes_after_load == 3

    # Make 2 more changes after reload
    await redis_client.execute_command('AM.PUTTEXT', 'test4', 'field4', 'after1')
    await redis_client.execute_command('AM.PUTTEXT', 'test4', 'field5', 'after2')

    changes_final = await redis_client.execute_command('AM.NUMCHANGES', 'test4')
    assert changes_final == 5


@pytest.mark.persistence
async def test_nested_paths_preservation(redis_client, clean_redis):
    """Test that nested path operations are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test5')

    # Create nested structure
    await redis_client.execute_command('AM.PUTTEXT', 'test5', 'user.name', 'Alice')
    await redis_client.execute_command('AM.PUTINT', 'test5', 'user.age', 30)
    await redis_client.execute_command('AM.PUTTEXT', 'test5', 'user.profile.bio', 'Hello World')

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test5')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test5')
    await redis_client.delete('test5')
    await redis_client.execute_command('AM.LOAD', 'test5', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test5')
    assert changes_before == changes_after == 3

    # Verify nested data
    assert await redis_client.execute_command('AM.GETTEXT', 'test5', 'user.name') == b'Alice'
    assert await redis_client.execute_command('AM.GETINT', 'test5', 'user.age') == 30
    assert await redis_client.execute_command('AM.GETTEXT', 'test5', 'user.profile.bio') == b'Hello World'


@pytest.mark.persistence
async def test_list_operations_preservation(redis_client, clean_redis):
    """Test that list operations are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test6')

    # Create and populate list
    await redis_client.execute_command('AM.CREATELIST', 'test6', 'items')
    await redis_client.execute_command('AM.APPENDTEXT', 'test6', 'items', 'item1')
    await redis_client.execute_command('AM.APPENDTEXT', 'test6', 'items', 'item2')
    await redis_client.execute_command('AM.APPENDTEXT', 'test6', 'items', 'item3')
    await redis_client.execute_command('AM.PUTTEXT', 'test6', 'items[1]', 'modified')

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test6')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test6')
    await redis_client.delete('test6')
    await redis_client.execute_command('AM.LOAD', 'test6', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test6')
    assert changes_before == changes_after == 5

    # Verify list data
    length = await redis_client.execute_command('AM.LISTLEN', 'test6', 'items')
    assert length == 3

    assert await redis_client.execute_command('AM.GETTEXT', 'test6', 'items[0]') == b'item1'
    assert await redis_client.execute_command('AM.GETTEXT', 'test6', 'items[1]') == b'modified'
    assert await redis_client.execute_command('AM.GETTEXT', 'test6', 'items[2]') == b'item3'


@pytest.mark.persistence
async def test_text_splice_preservation(redis_client, clean_redis):
    """Test that text splice operations are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test7')

    # Create text and splice it
    await redis_client.execute_command('AM.PUTTEXT', 'test7', 'content', 'Hello World')
    await redis_client.execute_command('AM.SPLICETEXT', 'test7', 'content', 6, 5, 'Redis')

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test7')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test7')
    await redis_client.delete('test7')
    await redis_client.execute_command('AM.LOAD', 'test7', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test7')
    assert changes_before == changes_after == 3  # puttext + 2 splices

    # Verify spliced content
    content = await redis_client.execute_command('AM.GETTEXT', 'test7', 'content')
    assert content == b'Hello Redis'


@pytest.mark.persistence
async def test_counter_operations_preservation(redis_client, clean_redis):
    """Test that counter operations are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test13')

    # Counter operations
    await redis_client.execute_command('AM.PUTCOUNTER', 'test13', 'visits', 0)
    await redis_client.execute_command('AM.INCCOUNTER', 'test13', 'visits', 1)
    await redis_client.execute_command('AM.INCCOUNTER', 'test13', 'visits', 5)
    await redis_client.execute_command('AM.INCCOUNTER', 'test13', 'visits', 3)

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test13')
    value_before = await redis_client.execute_command('AM.GETCOUNTER', 'test13', 'visits')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test13')
    await redis_client.delete('test13')
    await redis_client.execute_command('AM.LOAD', 'test13', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test13')
    value_after = await redis_client.execute_command('AM.GETCOUNTER', 'test13', 'visits')

    assert changes_before == changes_after == 4
    assert value_before == value_after == 9


@pytest.mark.persistence
async def test_timestamp_operations_preservation(redis_client, clean_redis):
    """Test that timestamp operations are preserved through save/load."""
    await redis_client.execute_command('AM.NEW', 'test14')

    # Timestamp operations
    await redis_client.execute_command('AM.PUTTIMESTAMP', 'test14', 'created', 1234567890000)
    await redis_client.execute_command('AM.PUTTIMESTAMP', 'test14', 'updated', 9876543210000)

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test14')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test14')
    await redis_client.delete('test14')
    await redis_client.execute_command('AM.LOAD', 'test14', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test14')
    assert changes_before == changes_after == 2

    # Verify timestamps
    created = await redis_client.execute_command('AM.GETTIMESTAMP', 'test14', 'created')
    updated = await redis_client.execute_command('AM.GETTIMESTAMP', 'test14', 'updated')

    assert created == 1234567890000
    assert updated == 9876543210000


@pytest.mark.persistence
async def test_empty_document_preservation(redis_client, clean_redis):
    """Test that an empty document can be saved and loaded."""
    await redis_client.execute_command('AM.NEW', 'test9')

    changes_before = await redis_client.execute_command('AM.NUMCHANGES', 'test9')
    assert changes_before == 0

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test9')
    await redis_client.delete('test9')
    await redis_client.execute_command('AM.LOAD', 'test9', saved_data)

    changes_after = await redis_client.execute_command('AM.NUMCHANGES', 'test9')
    assert changes_after == 0


@pytest.mark.persistence
async def test_multiple_save_load_cycles(redis_client, clean_redis):
    """Test that multiple save/load cycles preserve change history."""
    await redis_client.execute_command('AM.NEW', 'test10')

    # Cycle 1
    await redis_client.execute_command('AM.PUTTEXT', 'test10', 'field', 'cycle1')
    changes_1 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')

    saved_data = await redis_client.execute_command('AM.SAVE', 'test10')
    await redis_client.delete('test10')
    await redis_client.execute_command('AM.LOAD', 'test10', saved_data)

    changes_2 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')
    assert changes_1 == changes_2 == 1

    # Cycle 2
    await redis_client.execute_command('AM.PUTTEXT', 'test10', 'field', 'cycle2')
    changes_3 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')

    saved_data = await redis_client.execute_command('AM.SAVE', 'test10')
    await redis_client.delete('test10')
    await redis_client.execute_command('AM.LOAD', 'test10', saved_data)

    changes_4 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')
    assert changes_3 == changes_4 == 2

    # Cycle 3
    await redis_client.execute_command('AM.PUTTEXT', 'test10', 'field', 'cycle3')
    changes_5 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')

    saved_data = await redis_client.execute_command('AM.SAVE', 'test10')
    await redis_client.delete('test10')
    await redis_client.execute_command('AM.LOAD', 'test10', saved_data)

    changes_6 = await redis_client.execute_command('AM.NUMCHANGES', 'test10')
    assert changes_5 == changes_6 == 3


@pytest.mark.persistence
async def test_change_hash_consistency(redis_client, clean_redis):
    """Test that change hashes remain consistent through save/load."""
    await redis_client.execute_command('AM.NEW', 'test15')

    # Make some changes
    await redis_client.execute_command('AM.PUTTEXT', 'test15', 'field1', 'value1')
    await redis_client.execute_command('AM.PUTTEXT', 'test15', 'field2', 'value2')
    await redis_client.execute_command('AM.PUTTEXT', 'test15', 'field3', 'value3')

    # Get change hashes before save
    changes_before = await redis_client.execute_command('AM.CHANGES', 'test15')
    changes_count_before = await redis_client.execute_command('AM.NUMCHANGES', 'test15')

    # Save and reload
    saved_data = await redis_client.execute_command('AM.SAVE', 'test15')
    await redis_client.delete('test15')
    await redis_client.execute_command('AM.LOAD', 'test15', saved_data)

    # Get change hashes after load
    changes_after = await redis_client.execute_command('AM.CHANGES', 'test15')
    changes_count_after = await redis_client.execute_command('AM.NUMCHANGES', 'test15')

    # Verify same number of changes
    assert changes_count_before == changes_count_after == 3

    # Verify changes are a list of bytes
    assert isinstance(changes_before, list)
    assert isinstance(changes_after, list)
    assert len(changes_before) == len(changes_after) == 3

    # All changes should be binary data
    assert all(isinstance(c, bytes) for c in changes_before)
    assert all(isinstance(c, bytes) for c in changes_after)
