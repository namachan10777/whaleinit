#!/bin/sh

echo "msg: {{ env.TEST_MSG }}"

echo "Parent process spawned"
/usr/local/bin/test_child.sh &
echo "child process spawned"
sleep 1
echo "Parent process finished"
