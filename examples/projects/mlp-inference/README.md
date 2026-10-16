# MLP batch inference

This project loads a verified `training.train_mlp` checkpoint and evaluates a
bounded batch of input vectors with `training.predict_mlp`. The checkpoint is
read-only: the project grants no write capability and the input document must
contain an `inputs` JSON array with uniform numeric dimensions.

The inference result includes checkpoint and input hashes, the checkpoint
training step, dimensions and predictions. It is a bounded CPU profile, not a
general model-serving or HTTP API.
