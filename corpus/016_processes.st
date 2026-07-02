| sem log |
log := WriteStream on: (String new: 4).
sem := Semaphore new.
[log nextPutAll: 'w1.'. sem signal. log nextPutAll: 'w2.'] fork.
log nextPutAll: 'm1.'.
sem wait.
log nextPutAll: 'm2.'.
Processor yield.
log nextPutAll: 'm3.'.
Transcript showCr: log contents
