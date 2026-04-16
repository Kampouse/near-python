# near-python

A tiny Python-subset runtime compiled to WASM (wasm32-wasip2) for NEAR OutLayer. Write Python scripts that interact with NEAR blockchain — view contracts, send transactions, manage storage — all running as a WASM component.

**Binary size:** ~296KB | **Target:** wasm32-wasip2 (WASI Preview 2 Component)

## Features

### Python Syntax Supported

| Feature | Syntax | Example |
|---------|--------|---------|
| Variables | `x = 5` | `name = "NEAR"` |
| Arithmetic | `+`, `-`, `*`, `/`, `%` | `total = price * amount` |
| Comparisons | `==`, `!=`, `>`, `<`, `>=`, `<=` | `if balance > threshold:` |
| Boolean | `and`, `or`, `not` | `if active and not paused:` |
| Augmented assign | `+=`, `-=`, `*=`, `/=` | `total += amount` |
| If/elif/else | `if: ... elif: ... else: ...` | Conditional branching |
| For loops | `for x in range(n):` | `for i in range(10):` |
| While loops | `while cond: ...` | `while retries > 0:` |
| Break/Continue | `break`, `continue` | Loop control |
| Functions | `def name(args): ... return val` | User-defined functions |
| Lists | `[1, 2, 3]` | With `.append()`, `.pop()`, etc. |
| Dicts | `{"key": "val"}` | With `.keys()`, `.values()`, `.get()` |
| Indexing | `data["key"]`, `list[0]` | Negative indices supported |
| f-strings | `f"value: {var}"` | String interpolation |
| Try/Except | `try: ... except: ...` | Error handling |
| Comments | `# comment` | Single-line comments |

### Built-in Functions

| Function | Description |
|----------|-------------|
| `print(expr)` | Print to stdout |
| `len(x)` | Length of list, dict, or string |
| `range(n)` / `range(a, b)` / `range(a, b, step)` | Generate number range |
| `int(x)` | Convert to integer |
| `str(x)` | Convert to string |
| `type(x)` | Get type name |
| `json.dumps(x)` | Serialize to JSON string |
| `json.loads(s)` | Parse JSON string |

### String Methods

`split(sep)`, `replace(old, new)`, `strip()`, `lower()`, `upper()`, `startswith(prefix)`, `endswith(suffix)`, `find(needle)`, `join(list)`, `count(needle)`

### List Methods

`append(x)`, `extend(list)`, `insert(i, x)`, `pop(i)`, `reverse()`, `sort()`, `index(x)`

### Dict Methods

`keys()`, `values()`, `items()`, `get(key, default)`

### NEAR Blockchain API

#### View Functions (read-only)

```python
# Call a view function on a contract
result = near.view("contract.near", "method_name", {})
result = near.view("contract.near", "method_name", '{"key": "val"}')

# Get account info
account = near.view_account("account.near")

# Get block info
block = near.block("final")
block = near.block("12345")  # by height

# Get current block height
height = near.block_height()
```

#### Transactions (sign & send)

```python
# Call a contract method (write operation)
result = near.call(
    "signer.near",           # signer account ID
    "ed25519:PRIVATE_KEY",   # signer private key
    "receiver.near",          # receiver/contract
    "method_name",            # method to call
    '{"key": "val"}',        # args as JSON string
    "1000000000000000000",    # deposit in yoctoNEAR
    "300000000000000",        # gas
    "FINAL"                   # wait until (optional, default: "FINAL")
)
```

#### Storage (in-memory, persists during execution)

```python
# Store a value
near.storage.put("key", json.dumps({"data": 42}))

# Retrieve a value
value = near.storage.get("key")
if value:
    data = json.loads(value)
    print(data)
```

> **Note:** Storage is in-memory for the duration of script execution. For persistent storage, the host must provide a storage interface.

### HTTP Client (Stub)

```python
# HTTP is stubbed — returns empty JSON
price = http.get("https://api.example.com/price")
```

> **Note:** HTTP requires host support. Currently stubs return `{}` with a warning.

## Building

### Prerequisites

- Rust with `wasm32-wasip2` target: `rustup target add wasm32-wasip2`

### Build

```bash
cargo build --target wasm32-wasip2 --release
```

Binary output: `target/wasm32-wasip2/release/near-python.wasm`

## Running

Use the wasi-test-runner from [near-outlayer](https://github.com/Kampouse/near-outlayer):

```bash
cd near-outlayer/wasi-examples/wasi-test-runner
cargo build --release

# Run a script
./target/release/wasi-test \
  --wasm /path/to/near-python.wasm \
  --rpc --rpc-url "https://rpc.mainnet.near.org" \
  --input '{"script": "print(near.block_height())"}'
```

## Example Scripts

See the `examples/` directory:

- **`price_checker.py`** — Check NEAR price from on-chain DEX pools
- **`monitor_burrow.py`** — Monitor Burrow margin positions
- **`liquidation_bot.py`** — Find and report liquidatable positions
- **`arbitrage_scanner.py`** — Scan DEX pools for arbitrage opportunities
- **`dao_monitor.py`** — Watch DAO proposals and status

## Architecture

```
┌─────────────────────────────────────┐
│          Python Script              │
│  (variables, loops, functions,      │
│   near.view(), near.call(), etc.)   │
└──────────────┬──────────────────────┘
               │
               ▼
┌─────────────────────────────────────┐
│       near-python (WASM)            │
│  ┌─────────────┐  ┌──────────────┐ │
│  │   Parser    │  │ Interpreter  │ │
│  │ (line-based)│→ │  (tree-walk) │ │
│  └─────────────┘  └──────┬───────┘ │
│                          │         │
│  ┌───────────────────────┘         │
│  │ Built-in Functions               │
│  │ • NEAR RPC (via WIT imports)     │
│  │ • In-memory Storage              │
│  │ • JSON (serde_json)              │
│  │ • List/Dict/String methods       │
│  └─────────────────────────────────┘
└──────────────┬──────────────────────┘
               │ WIT Interface (near:rpc/api)
               ▼
┌─────────────────────────────────────┐
│       Host Runtime                  │
│  (wasi-test-runner / OutLayer)      │
│  • JSON-RPC proxy to NEAR          │
│  • Transaction signing              │
│  • Rate limiting                    │
└─────────────────────────────────────┘
```

## WIT Interface

The WASM component imports from `near:rpc/api`:

```wit
package near:rpc@0.1.0;

interface api {
    view: func(contract-id: string, method-name: string, args-json: string, finality-or-block: string) -> tuple<string, string>;
    view-account: func(account-id: string, finality-or-block: string) -> tuple<string, string>;
    block: func(finality-or-block: string) -> tuple<string, string>;
    call: func(signer-id: string, signer-key: string, receiver-id: string, method-name: string, args-json: string, deposit-yocto: string, gas: string, wait-until: string) -> tuple<string, string>;
    // ... and more (transfer, gas-price, status, etc.)
}

world rpc-host {
    import api;
}
```

All host functions return `(result_string, error_string)`. If `error_string` is non-empty, the call failed.

## Deploying to OutLayer

1. Build the WASM binary
2. Upload to OutLayer as a component
3. Configure RPC URL and signer keys
4. Schedule execution via cron or triggers

```bash
# Build
cargo build --target wasm32-wasip2 --release

# The binary is ready at:
ls -lh target/wasm32-wasip2/release/near-python.wasm
```

## Limitations

- **Single-file:** All code in `src/main.rs` (intentional for simplicity)
- **Line-based parser:** No multi-line expressions or complex nesting beyond what's documented
- **Storage is in-memory:** Persists only during single execution
- **HTTP is stubbed:** Requires host implementation for real HTTP
- **No classes:** Only functions, no class definitions
- **No imports:** No module system (all built-ins are available globally)
- **No async:** Synchronous execution only

## License

MIT
