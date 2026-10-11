<?php

// Answers after a delay, so the request is still in flight when the plugin looks at it.
usleep(((int) ($_GET['ms'] ?? 300)) * 1000);
header('Content-Type: application/json');
echo '{"slow":true}';
