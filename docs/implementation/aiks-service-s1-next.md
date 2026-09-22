# Service extraction continuation

Current implementation, checkpoints and explicit rulings are in `aiks-service-s1-progress.md`; local startup, model configuration, rollback and limitations are in `aiks-service-s1.md`.

Tasks 9–10 have now been implemented. Do not repeat older Task 6 or Task 8 patches. Inspect the exact current branch HEAD and its Actions before claiming final verification. No automatic merge to main or Release is authorized.

The next product phase after the verified S1 local workflow and real-machine acceptance is S2: authenticated team principals/spaces/memberships, restricted content read/write mapping, share publication and per-resource authorization, with the SiYuan content service internal only. S3 handles remote/local deployment and installation packaging; S4 adds cited knowledge-base Q&A. A currently loopback-only Service must not be opened publicly by removing its personal-mode validation.
