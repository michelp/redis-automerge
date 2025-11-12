"""
Advanced concurrent synchronization tests for Redis-Automerge.

Tests complex scenarios with multiple documents syncing concurrently.
"""
import pytest
import asyncio


@pytest.mark.sync
@pytest.mark.concurrent
async def test_concurrent_sync_to_multiple_targets(redis_client, clean_redis):
    """Test syncing from one source to multiple targets concurrently."""
    # Create source with data
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'field', 'value')
    await redis_client.execute_command('AM.PUTINT', 'source', 'count', 42)

    changes = await redis_client.execute_command('AM.CHANGES', 'source')

    # Create 5 target documents
    targets = [f'target_{i}' for i in range(5)]

    # Create all targets concurrently
    await asyncio.gather(*[
        redis_client.execute_command('AM.NEW', target)
        for target in targets
    ])

    # Apply changes to all targets concurrently
    await asyncio.gather(*[
        redis_client.execute_command('AM.APPLY', target, *changes)
        for target in targets
    ])

    # Verify all targets have the same data
    results = await asyncio.gather(*[
        redis_client.execute_command('AM.GETTEXT', target, 'field')
        for target in targets
    ])

    assert all(r == b'value' for r in results)


@pytest.mark.sync
@pytest.mark.concurrent
async def test_network_partition_simulation(redis_client, clean_redis):
    """Simulate a network partition where some documents can't sync initially."""
    # Create three documents
    await redis_client.execute_command('AM.NEW', 'node_a')
    await redis_client.execute_command('AM.NEW', 'node_b')
    await redis_client.execute_command('AM.NEW', 'node_c')

    # A and B can communicate, C is partitioned
    await redis_client.execute_command('AM.PUTTEXT', 'node_a', 'from_a', 'data_a')
    await redis_client.execute_command('AM.PUTTEXT', 'node_b', 'from_b', 'data_b')
    await redis_client.execute_command('AM.PUTTEXT', 'node_c', 'from_c', 'data_c')

    # A and B sync with each other (C is isolated)
    changes_a = await redis_client.execute_command('AM.CHANGES', 'node_a')
    changes_b = await redis_client.execute_command('AM.CHANGES', 'node_b')

    await asyncio.gather(
        redis_client.execute_command('AM.APPLY', 'node_b', *changes_a),
        redis_client.execute_command('AM.APPLY', 'node_a', *changes_b)
    )

    # Verify A and B have each other's data
    val_a_on_b = await redis_client.execute_command('AM.GETTEXT', 'node_b', 'from_a')
    assert val_a_on_b == b'data_a'

    # Verify C's data is not on B yet
    val_c_on_b = await redis_client.execute_command('AM.GETTEXT', 'node_b', 'from_c')
    assert val_c_on_b is None

    # "Network partition heals" - C rejoins
    changes_c = await redis_client.execute_command('AM.CHANGES', 'node_c')
    changes_a_updated = await redis_client.execute_command('AM.CHANGES', 'node_a')
    changes_b_updated = await redis_client.execute_command('AM.CHANGES', 'node_b')

    # C syncs with A and B
    await asyncio.gather(
        redis_client.execute_command('AM.APPLY', 'node_c', *changes_a_updated),
        redis_client.execute_command('AM.APPLY', 'node_c', *changes_b_updated),
        redis_client.execute_command('AM.APPLY', 'node_a', *changes_c),
        redis_client.execute_command('AM.APPLY', 'node_b', *changes_c)
    )

    # All nodes should have all data (eventual consistency)
    for node in ['node_a', 'node_b', 'node_c']:
        val_a = await redis_client.execute_command('AM.GETTEXT', node, 'from_a')
        val_b = await redis_client.execute_command('AM.GETTEXT', node, 'from_b')
        val_c = await redis_client.execute_command('AM.GETTEXT', node, 'from_c')

        assert val_a == b'data_a'
        assert val_b == b'data_b'
        assert val_c == b'data_c'


@pytest.mark.sync
@pytest.mark.concurrent
async def test_concurrent_bidirectional_sync_multiple_pairs(redis_client, clean_redis):
    """Test multiple document pairs syncing bidirectionally at the same time."""
    # Create pairs of documents
    pairs = [
        ('doc_a1', 'doc_a2'),
        ('doc_b1', 'doc_b2'),
        ('doc_c1', 'doc_c2'),
    ]

    # Create all documents
    for doc1, doc2 in pairs:
        await redis_client.execute_command('AM.NEW', doc1)
        await redis_client.execute_command('AM.NEW', doc2)

    # Each pair gets different data
    for idx, (doc1, doc2) in enumerate(pairs):
        await redis_client.execute_command('AM.PUTTEXT', doc1, 'field', f'from_{doc1}')
        await redis_client.execute_command('AM.PUTTEXT', doc2, 'field', f'from_{doc2}')

    # Sync all pairs concurrently
    async def sync_pair(doc1, doc2):
        changes_1 = await redis_client.execute_command('AM.CHANGES', doc1)
        changes_2 = await redis_client.execute_command('AM.CHANGES', doc2)

        await asyncio.gather(
            redis_client.execute_command('AM.APPLY', doc2, *changes_1),
            redis_client.execute_command('AM.APPLY', doc1, *changes_2)
        )

    await asyncio.gather(*[
        sync_pair(doc1, doc2) for doc1, doc2 in pairs
    ])

    # Verify each pair converged
    for doc1, doc2 in pairs:
        val1 = await redis_client.execute_command('AM.GETTEXT', doc1, 'field')
        val2 = await redis_client.execute_command('AM.GETTEXT', doc2, 'field')

        # Both should have same value (LWW conflict resolution)
        assert val1 == val2


@pytest.mark.sync
@pytest.mark.concurrent
async def test_star_topology_sync(redis_client, clean_redis):
    """Test sync in a star topology: one central hub syncing with multiple spokes."""
    # Create hub and spokes
    await redis_client.execute_command('AM.NEW', 'hub')
    spokes = [f'spoke_{i}' for i in range(4)]

    for spoke in spokes:
        await redis_client.execute_command('AM.NEW', spoke)
        await redis_client.execute_command('AM.PUTTEXT', spoke, f'data_from_{spoke}', 'value')

    # All spokes sync their changes to hub concurrently
    async def sync_spoke_to_hub(spoke):
        changes = await redis_client.execute_command('AM.CHANGES', spoke)
        await redis_client.execute_command('AM.APPLY', 'hub', *changes)

    await asyncio.gather(*[
        sync_spoke_to_hub(spoke) for spoke in spokes
    ])

    # Hub should have data from all spokes
    for spoke in spokes:
        val = await redis_client.execute_command('AM.GETTEXT', 'hub', f'data_from_{spoke}')
        assert val == b'value'

    # Now hub syncs back to all spokes concurrently
    hub_changes = await redis_client.execute_command('AM.CHANGES', 'hub')

    await asyncio.gather(*[
        redis_client.execute_command('AM.APPLY', spoke, *hub_changes)
        for spoke in spokes
    ])

    # All spokes should now have data from all other spokes
    for spoke in spokes:
        for other_spoke in spokes:
            val = await redis_client.execute_command('AM.GETTEXT', spoke, f'data_from_{other_spoke}')
            assert val == b'value'


@pytest.mark.sync
@pytest.mark.concurrent
@pytest.mark.slow
async def test_eventual_consistency_stress(redis_client, clean_redis):
    """Stress test: many documents eventually reaching consistency."""
    num_docs = 6
    docs = [f'doc_{i}' for i in range(num_docs)]

    # Create all documents
    await asyncio.gather(*[
        redis_client.execute_command('AM.NEW', doc)
        for doc in docs
    ])

    # Each document gets unique data
    await asyncio.gather(*[
        redis_client.execute_command('AM.PUTTEXT', doc, f'field_from_{doc}', f'value_{i}')
        for i, doc in enumerate(docs)
    ])

    # Multiple rounds of random pairwise syncing
    async def sync_documents(doc1, doc2):
        changes_1 = await redis_client.execute_command('AM.CHANGES', doc1)
        changes_2 = await redis_client.execute_command('AM.CHANGES', doc2)

        await asyncio.gather(
            redis_client.execute_command('AM.APPLY', doc2, *changes_1),
            redis_client.execute_command('AM.APPLY', doc1, *changes_2)
        )

    # Sync all pairs (full mesh)
    for i in range(num_docs):
        for j in range(i + 1, num_docs):
            await sync_documents(docs[i], docs[j])

    # All documents should have all fields (eventual consistency)
    for doc in docs:
        for other_doc in docs:
            field_name = f'field_from_{other_doc}'
            val = await redis_client.execute_command('AM.GETTEXT', doc, field_name)
            assert val is not None  # Should have data from all docs


@pytest.mark.sync
@pytest.mark.concurrent
async def test_concurrent_counter_sync(redis_client, clean_redis):
    """Test that concurrent counter increments sync correctly across documents."""
    # Create two documents with shared counter
    await redis_client.execute_command('AM.NEW', 'doc1')
    await redis_client.execute_command('AM.PUTCOUNTER', 'doc1', 'counter', 0)

    # Sync to doc2
    changes = await redis_client.execute_command('AM.CHANGES', 'doc1')
    await redis_client.execute_command('AM.NEW', 'doc2')
    await redis_client.execute_command('AM.APPLY', 'doc2', *changes)

    # Both increment concurrently while offline
    await asyncio.gather(
        redis_client.execute_command('AM.INCCOUNTER', 'doc1', 'counter', 10),
        redis_client.execute_command('AM.INCCOUNTER', 'doc2', 'counter', 15)
    )

    # Sync bidirectionally
    changes_1 = await redis_client.execute_command('AM.CHANGES', 'doc1')
    changes_2 = await redis_client.execute_command('AM.CHANGES', 'doc2')

    await asyncio.gather(
        redis_client.execute_command('AM.APPLY', 'doc2', *changes_1),
        redis_client.execute_command('AM.APPLY', 'doc1', *changes_2)
    )

    # Both should converge to sum: 0 + 10 + 15 = 25
    val1 = await redis_client.execute_command('AM.GETCOUNTER', 'doc1', 'counter')
    val2 = await redis_client.execute_command('AM.GETCOUNTER', 'doc2', 'counter')

    assert val1 == 25
    assert val2 == 25


@pytest.mark.sync
@pytest.mark.concurrent
async def test_ring_topology_sync(redis_client, clean_redis):
    """Test sync in a ring topology: each node syncs to its neighbor."""
    nodes = [f'node_{i}' for i in range(5)]

    # Create all nodes
    await asyncio.gather(*[
        redis_client.execute_command('AM.NEW', node)
        for node in nodes
    ])

    # Each node gets unique data
    for i, node in enumerate(nodes):
        await redis_client.execute_command('AM.PUTTEXT', node, f'data_{i}', f'value_{i}')

    # Sync in a ring: node_0 -> node_1 -> node_2 -> ... -> node_0
    for i in range(len(nodes)):
        current = nodes[i]
        next_node = nodes[(i + 1) % len(nodes)]

        changes = await redis_client.execute_command('AM.CHANGES', current)
        await redis_client.execute_command('AM.APPLY', next_node, *changes)

    # After one round, each node should have data from its predecessor
    # Continue syncing multiple rounds until all data propagates
    for round_num in range(len(nodes)):
        for i in range(len(nodes)):
            current = nodes[i]
            next_node = nodes[(i + 1) % len(nodes)]

            changes = await redis_client.execute_command('AM.CHANGES', current)
            if changes:  # Only apply if there are changes
                await redis_client.execute_command('AM.APPLY', next_node, *changes)

    # Eventually all nodes should have all data
    for node in nodes:
        for i in range(len(nodes)):
            val = await redis_client.execute_command('AM.GETTEXT', node, f'data_{i}')
            assert val == f'value_{i}'.encode()


@pytest.mark.sync
@pytest.mark.concurrent
async def test_concurrent_sync_with_persistence(redis_client, clean_redis):
    """Test that documents synced concurrently can all be persisted correctly."""
    # Create source
    await redis_client.execute_command('AM.NEW', 'source')
    await redis_client.execute_command('AM.PUTTEXT', 'source', 'data', 'shared_value')

    changes = await redis_client.execute_command('AM.CHANGES', 'source')

    # Create and sync multiple targets concurrently
    targets = [f'target_{i}' for i in range(3)]

    await asyncio.gather(*[
        redis_client.execute_command('AM.NEW', target)
        for target in targets
    ])

    await asyncio.gather(*[
        redis_client.execute_command('AM.APPLY', target, *changes)
        for target in targets
    ])

    # Save all targets concurrently
    saved_data = await asyncio.gather(*[
        redis_client.execute_command('AM.SAVE', target)
        for target in targets
    ])

    # Delete and reload all targets
    await asyncio.gather(*[
        redis_client.delete(target)
        for target in targets
    ])

    await asyncio.gather(*[
        redis_client.execute_command('AM.LOAD', target, data)
        for target, data in zip(targets, saved_data)
    ])

    # Verify all reloaded correctly
    results = await asyncio.gather(*[
        redis_client.execute_command('AM.GETTEXT', target, 'data')
        for target in targets
    ])

    assert all(r == b'shared_value' for r in results)


@pytest.mark.sync
@pytest.mark.concurrent
async def test_cascading_sync(redis_client, clean_redis):
    """Test cascading sync: A -> B -> C."""
    # Create three documents
    await redis_client.execute_command('AM.NEW', 'doc_a')
    await redis_client.execute_command('AM.NEW', 'doc_b')
    await redis_client.execute_command('AM.NEW', 'doc_c')

    # A gets original data
    await redis_client.execute_command('AM.PUTTEXT', 'doc_a', 'field', 'original')

    # A -> B
    changes_a = await redis_client.execute_command('AM.CHANGES', 'doc_a')
    await redis_client.execute_command('AM.APPLY', 'doc_b', *changes_a)

    # B -> C
    changes_b = await redis_client.execute_command('AM.CHANGES', 'doc_b')
    await redis_client.execute_command('AM.APPLY', 'doc_c', *changes_b)

    # C should have the data that originated from A
    val = await redis_client.execute_command('AM.GETTEXT', 'doc_c', 'field')
    assert val == b'original'

    # All three should have same change count
    count_a = await redis_client.execute_command('AM.NUMCHANGES', 'doc_a')
    count_b = await redis_client.execute_command('AM.NUMCHANGES', 'doc_b')
    count_c = await redis_client.execute_command('AM.NUMCHANGES', 'doc_c')

    assert count_a == count_b == count_c == 1


@pytest.mark.sync
@pytest.mark.concurrent
async def test_sync_convergence_verification(redis_client, clean_redis):
    """Verify that all synced documents have identical state (convergence test)."""
    docs = ['doc1', 'doc2', 'doc3']

    # Create all documents
    await asyncio.gather(*[
        redis_client.execute_command('AM.NEW', doc)
        for doc in docs
    ])

    # Each adds different data
    await redis_client.execute_command('AM.PUTTEXT', 'doc1', 'field1', 'value1')
    await redis_client.execute_command('AM.PUTTEXT', 'doc2', 'field2', 'value2')
    await redis_client.execute_command('AM.PUTTEXT', 'doc3', 'field3', 'value3')

    # Full mesh sync
    async def full_mesh_sync():
        for i, doc_i in enumerate(docs):
            for j, doc_j in enumerate(docs):
                if i != j:
                    changes = await redis_client.execute_command('AM.CHANGES', doc_i)
                    await redis_client.execute_command('AM.APPLY', doc_j, *changes)

    # Run full mesh sync
    await full_mesh_sync()

    # All documents should have identical state
    # Export to JSON and compare
    json_outputs = await asyncio.gather(*[
        redis_client.execute_command('AM.TOJSON', doc)
        for doc in docs
    ])

    # All JSON outputs should be identical (convergence!)
    assert all(j == json_outputs[0] for j in json_outputs)
