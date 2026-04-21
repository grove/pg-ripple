#!/bin/bash
# Waiting for the background command to finish isn't directly possible if I don't have the pid
# but I can check if cargo processes are running.
ps aux | grep cargo | grep pgrx | grep regress
