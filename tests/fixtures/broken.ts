// This file has intentional parse errors for testing error recovery

const valid = process.env.VALID_VAR;

// Syntax error below
import { foo } from ;

const another = 42;
