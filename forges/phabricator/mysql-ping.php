#!/usr/bin/env php
<?php
/**
 * Simple PHP script to check MySQL connectivity
 * Returns exit code 0 if successful, 1 if failed
 */

$host = isset($argv[1]) ? $argv[1] : 'localhost';
$user = isset($argv[2]) ? $argv[2] : 'root';
$pass = isset($argv[3]) ? $argv[3] : '';

$link = @mysqli_connect($host, $user, $pass);
if ($link) {
  mysqli_close($link);
  echo "MySQL connection successful to {$host}\n";
  exit(0);
} else {
  $error = mysqli_connect_error();
  echo "MySQL connection failed to {$host}: {$error}\n";
  exit(1);
}
