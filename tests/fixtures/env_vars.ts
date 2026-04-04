// Test fixture: environment variable access patterns

// Direct access
const dbUrl = process.env.DATABASE_URL;
const stripeKey = process.env.STRIPE_SECRET_KEY;

// Bracket access
const apiKey = process.env["API_KEY"];

// Destructuring
const { NODE_ENV, LOG_LEVEL } = process.env;

// Default/fallback
const port = process.env.PORT || 3000;
const timeout = process.env.API_TIMEOUT ?? "5000";

// import.meta.env (Vite)
const viteUrl = import.meta.env.VITE_API_URL;

// Optional chaining
const optVal = process.env?.OPTIONAL_VAR;
