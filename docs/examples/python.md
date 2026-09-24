# Python Examples

## Installation

```bash
pip install requests stellar-sdk
```

## Depositing Collateral

```python
import requests
from typing import Dict, Any

API_BASE = "https://api.stellarlend.io/api"
TOKEN = "your-jwt-token"

def deposit_collateral(asset: str, amount: str) -> Dict[str, Any]:
    """Deposit collateral into the protocol."""
    headers = {
        "Authorization": f"Bearer {TOKEN}",
        "Content-Type": "application/json",
    }
    
    payload = {
        "asset": asset,
        "amount": amount,
    }
    
    response = requests.post(
        f"{API_BASE}/v1/lending/deposit",
        json=payload,
        headers=headers,
    )
    
    response.raise_for_status()
    return response.json()

# Usage
result = deposit_collateral("USDC", "1000.00")
print(result)
```

## Borrowing Assets

```python
def borrow_assets(asset: str, amount: str) -> Dict[str, Any]:
    """Borrow assets against collateral."""
    headers = {
        "Authorization": f"Bearer {TOKEN}",
        "Content-Type": "application/json",
    }
    
    payload = {
        "asset": asset,
        "amount": amount,
    }
    
    response = requests.post(
        f"{API_BASE}/v1/lending/borrow",
        json=payload,
        headers=headers,
    )
    
    response.raise_for_status()
    return response.json()

# Usage
result = borrow_assets("USDT", "500.00")
print(f"Borrowed: {result}")
```

## Checking Position

```python
def get_position(user_address: str) -> Dict[str, Any]:
    """Get lending position for a user."""
    headers = {
        "Authorization": f"Bearer {TOKEN}",
    }
    
    response = requests.get(
        f"{API_BASE}/v1/lending/positions/{user_address}",
        headers=headers,
    )
    
    response.raise_for_status()
    data = response.json()
    
    for position in data.get("positions", []):
        print(f"Collateral: {position['collateral']}")
        print(f"Borrowed: {position['borrowed']}")
        print(f"Health Factor: {position['healthFactor']}")
    
    return data

# Usage
positions = get_position("GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX")
```

## Multi-Step Transaction

```python
def create_multi_step_transaction(user_address: str) -> Dict[str, Any]:
    """Create a multi-step transaction."""
    headers = {
        "Authorization": f"Bearer {TOKEN}",
        "Content-Type": "application/json",
    }
    
    payload = {
        "userAddress": user_address,
        "description": "Swap and deposit",
        "steps": [
            {
                "operation": "swap",
                "amount": "100",
                "assetAddress": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            },
            {
                "operation": "deposit",
                "amount": "100",
                "assetAddress": "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            },
        ],
        "ttlSeconds": 600,
    }
    
    response = requests.post(
        f"{API_BASE}/transactions",
        json=payload,
        headers=headers,
    )
    
    response.raise_for_status()
    return response.json()
```

## Stellar Signature Authentication

```python
from stellar_sdk import Keypair
from datetime import datetime
import base64
import json

def create_stellar_signature(
    method: str,
    url: str,
    body: Dict[str, Any],
    secret_key: str,
) -> tuple[str, str]:
    """Create a Stellar signature for authentication."""
    keypair = Keypair.from_secret(secret_key)
    timestamp = str(int(datetime.now().timestamp() * 1000))
    
    payload = f"{method}:{url}:{json.dumps(body)}:{timestamp}"
    signature = base64.b64encode(
        keypair.sign(payload.encode())
    ).decode()
    
    return signature, timestamp

def create_credit_line(
    delegate_address: str,
    max_amount: str,
    secret_key: str,
) -> Dict[str, Any]:
    """Create a credit line with Stellar signature."""
    keypair = Keypair.from_secret(secret_key)
    user_address = keypair.public_key
    
    body = {
        "delegateAddress": delegate_address,
        "maxAmount": max_amount,
        "interestRate": "0.06",
        "maturityDate": datetime.now().isoformat() + "Z",
    }
    
    signature, timestamp = create_stellar_signature(
        "POST",
        "/api/credit/create",
        body,
        secret_key,
    )
    
    headers = {
        "X-User-Address": user_address,
        "X-Stellar-Signature": signature,
        "X-Payload-Timestamp": timestamp,
        "Content-Type": "application/json",
    }
    
    response = requests.post(
        f"{API_BASE}/credit/create",
        json=body,
        headers=headers,
    )
    
    response.raise_for_status()
    return response.json()
```

## Error Handling

```python
import requests
from requests.exceptions import RequestException

def handle_api_error(error: RequestException):
    """Handle API errors gracefully."""
    if isinstance(error, requests.exceptions.HTTPError):
        status = error.response.status_code
        data = error.response.json()
        
        if status == 401:
            print("Unauthorized: Check your token")
        elif status == 403:
            print("Forbidden: Insufficient permissions")
        elif status == 404:
            print(f"Not found: {data.get('error')}")
        elif status == 429:
            print("Rate limited: Too many requests")
        else:
            print(f"Error: {data.get('error')}")
    else:
        print(f"Request failed: {error}")

def safe_api_call(func, *args, **kwargs):
    """Execute API call with error handling."""
    try:
        return func(*args, **kwargs)
    except RequestException as e:
        handle_api_error(e)
        return None
```

## Complete Example: Lending Client

```python
class StellarLendClient:
    """Client for interacting with StellarLend protocol."""
    
    def __init__(self, token: str, user_address: str):
        self.token = token
        self.user_address = user_address
        self.headers = {
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        }
    
    def deposit(self, asset: str, amount: str) -> Dict[str, Any]:
        """Deposit collateral."""
        payload = {"asset": asset, "amount": amount}
        response = requests.post(
            f"{API_BASE}/v1/lending/deposit",
            json=payload,
            headers=self.headers,
        )
        response.raise_for_status()
        return response.json()
    
    def borrow(self, asset: str, amount: str) -> Dict[str, Any]:
        """Borrow assets."""
        payload = {"asset": asset, "amount": amount}
        response = requests.post(
            f"{API_BASE}/v1/lending/borrow",
            json=payload,
            headers=self.headers,
        )
        response.raise_for_status()
        return response.json()
    
    def repay(self, asset: str, amount: str) -> Dict[str, Any]:
        """Repay borrowed assets."""
        payload = {"asset": asset, "amount": amount}
        response = requests.post(
            f"{API_BASE}/v1/lending/repay",
            json=payload,
            headers=self.headers,
        )
        response.raise_for_status()
        return response.json()
    
    def get_position(self) -> Dict[str, Any]:
        """Get current lending position."""
        response = requests.get(
            f"{API_BASE}/v1/lending/positions/{self.user_address}",
            headers=self.headers,
        )
        response.raise_for_status()
        return response.json()
    
    def get_health_factor(self) -> float:
        """Get health factor for position."""
        position = self.get_position()
        if position.get("positions"):
            return position["positions"][0].get("healthFactor", 0)
        return 0

# Usage Example
def main():
    client = StellarLendClient(
        token="your-jwt-token",
        user_address="GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
    )
    
    # Deposit collateral
    print("Depositing 1000 USDC...")
    client.deposit("USDC", "1000")
    
    # Check position
    print("Checking position...")
    position = client.get_position()
    print(f"Position: {position}")
    
    # Borrow
    print("Borrowing 500 USDT...")
    client.borrow("USDT", "500")
    
    # Check health factor
    health = client.get_health_factor()
    print(f"Health Factor: {health}")
    
    # Repay
    print("Repaying 250 USDT...")
    client.repay("USDT", "250")

if __name__ == "__main__":
    main()
```
