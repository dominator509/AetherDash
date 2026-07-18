import asyncio

import pytest

from server.swarm.scratchpad import Scratchpad


@pytest.mark.asyncio
async def test_concurrent_duplicates_are_stored_once() -> None:
    pad = Scratchpad()
    results = await asyncio.gather(
        *(
            pad.append(f"worker-{index}", " same   finding ", ("brain-1",))
            for index in range(20)
        )
    )
    assert sum(results) == 1
    entries = await pad.snapshot()
    assert len(entries) == 1
    assert entries[0].text == "same finding"


@pytest.mark.asyncio
async def test_entry_and_byte_bounds_are_hard() -> None:
    pad = Scratchpad(max_entries=2, max_bytes=10)
    assert await pad.append("one", "12345", ("brain-1",))
    assert await pad.append("two", "67890", ("brain-2",))
    assert not await pad.append("three", "x", ("brain-3",))
    assert await pad.size_bytes() == 10
    assert len(await pad.snapshot()) == 2


@pytest.mark.asyncio
async def test_uncited_content_is_rejected() -> None:
    pad = Scratchpad()
    with pytest.raises(ValueError, match="citations"):
        await pad.append("worker", "unsupported", ())
