#!/usr/bin/env python3
"""Regression proof against a private server: virtual fluid namespaces survive interleaving."""
import json
import os
from pathlib import Path
import time
import uuid

root = Path(os.environ['FUZZ_DIR'])


def exchange(request, fluid=True, error=False):
    name = ('fluid-host-' if fluid else 'namespace-test-') + str(uuid.uuid4()) + '.json'
    tmp = root / 'inbox' / (name + '.tmp')
    tmp.write_text(json.dumps(request))
    tmp.rename(root / 'inbox' / name)
    output = root / 'outbox' / name
    deadline = time.monotonic() + 60
    while not output.exists():
        if time.monotonic() > deadline:
            raise TimeoutError(name)
        time.sleep(.001)
    result = json.loads(output.read_text())
    output.unlink()
    if error:
        assert isinstance(result, dict) and 'error' in result, result
    else:
        assert not isinstance(result, dict) or 'error' not in result, result
    return result


region = [[x, y, z] for x in range(5) for y in range(5) for z in range(5)]
changes = [{'pos': p, 'state': 'minecraft:stone'} for p in region
           if p[1] == 0 or p[0] in (0, 4) or p[2] in (0, 4)]
changes.append({'pos': [2, 1, 2], 'state': 'minecraft:water[level=0]'})


def frame(owner, clock):
    request = dict(owner=owner, clock=clock, changes=changes if clock == 1 else [],
                   native=[[2, 1, 2]] if clock == 1 else [], manual=[], region=region)
    if clock == 1:
        request['begin'] = True
    return exchange(request)


def close(owner):
    exchange(dict(owner=owner, close=True))


alone = 'probe-' + str(uuid.uuid4())
expected = [frame(alone, clock) for clock in range(1, 21)]
close(alone)
a, b = ['probe-' + str(uuid.uuid4()) for _ in range(2)]
try:
    for clock in range(1, 21):
        assert frame(a, clock) == expected[clock - 1], ('first peer', clock)
        # Exercise ordinary case cleanup and namespace reset while both peers exist.
        result = exchange([dict(version=3, seed=0, ticks=2, blocks=[], events=[])], False)
        assert len(result) == 1
        if clock == 10:
            # A failed third namespace must be disposed without invalidating either survivor.
            exchange(dict(owner='fault-' + str(uuid.uuid4()), begin=True, clock=1,
                          changes=[dict(pos=[0, 1, 2], state='minecraft:water[level=0]')],
                          native=[], manual=[], region=region), error=True)
        assert frame(b, clock) == expected[clock - 1], ('second peer', clock)
finally:
    close(a)
    close(b)
print('20 frames: alone == two interleaved peers, with ordinary batches between every frame')
