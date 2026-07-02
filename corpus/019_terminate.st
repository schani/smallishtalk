| log w |
log := WriteStream on: (String new: 4).
w := [[log nextPutAll: 'work.'. Semaphore new wait] ensure: [log nextPutAll: 'cleanup.']] fork.
Processor yield.
w terminate.
Processor yield.
log nextPutAll: 'done.'.
Transcript showCr: log contents
