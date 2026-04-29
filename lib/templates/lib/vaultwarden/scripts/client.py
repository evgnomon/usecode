#!/usr/bin/env python3
"""Vaultwarden Python client for querying vault items (login credentials)."""

import argparse
import base64
import hashlib
import hmac
import json
import sys
from getpass import getpass
from urllib.parse import urljoin

import requests
from cryptography.hazmat.primitives.asymmetric import padding as asym_padding
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives import hashes, padding, serialization
from cryptography.hazmat.primitives.kdf.hkdf import HKDFExpand
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC


class VaultwardenClient:
    def __init__(self, base_url: str, verify_ssl: bool = False):
        self.base_url = base_url.rstrip("/")
        self.session = requests.Session()
        self.session.verify = verify_ssl
        self.access_token = None
        self._enc_key = None
        self._mac_key = None
        self._org_keys = {}  # org_id -> (enc_key, mac_key)
        self._private_key = None  # RSA private key for org key decryption

    def _api(self, path: str) -> str:
        return urljoin(self.base_url + "/", path)

    # --- Key derivation ---

    @staticmethod
    def _make_master_key(password: str, email: str, kdf_iterations: int) -> bytes:
        kdf = PBKDF2HMAC(
            algorithm=hashes.SHA256(),
            length=32,
            salt=email.lower().encode("utf-8"),
            iterations=kdf_iterations,
        )
        return kdf.derive(password.encode("utf-8"))

    @staticmethod
    def _make_master_password_hash(master_key: bytes, password: str) -> bytes:
        kdf = PBKDF2HMAC(
            algorithm=hashes.SHA256(),
            length=32,
            salt=password.encode("utf-8"),
            iterations=1,
        )
        return kdf.derive(master_key)

    @staticmethod
    def _stretch_key(key: bytes) -> tuple[bytes, bytes]:
        enc = HKDFExpand(algorithm=hashes.SHA256(), length=32, info=b"enc").derive(key)
        mac = HKDFExpand(algorithm=hashes.SHA256(), length=32, info=b"mac").derive(key)
        return enc, mac

    # --- Bitwarden CipherString decryption ---

    def _decrypt(
        self,
        cipher_string: str | None,
        enc_key: bytes | None = None,
        mac_key: bytes | None = None,
    ) -> str:
        if not cipher_string:
            return ""
        if enc_key is None:
            enc_key = self._enc_key
        if mac_key is None:
            mac_key = self._mac_key

        e_type, data = cipher_string.split(".", 1)
        e_type = int(e_type)
        if e_type != 2:
            raise ValueError(f"Unsupported encryption type: {e_type}")

        parts = data.split("|")
        iv = base64.b64decode(parts[0])
        ct = base64.b64decode(parts[1])
        mac = base64.b64decode(parts[2]) if len(parts) > 2 else None

        if mac is not None:
            mac_data = iv + ct
            expected = hmac.new(mac_key, mac_data, hashlib.sha256).digest()
            if not hmac.compare_digest(mac, expected):
                raise ValueError("MAC verification failed")

        cipher = Cipher(algorithms.AES(enc_key), modes.CBC(iv))
        decryptor = cipher.decryptor()
        padded = decryptor.update(ct) + decryptor.finalize()
        unpadder = padding.PKCS7(128).unpadder()
        return (unpadder.update(padded) + unpadder.finalize()).decode("utf-8")

    def _decrypt_symmetric_key(self, enc_key_str: str, master_key: bytes) -> None:
        m_enc, m_mac = self._stretch_key(master_key)

        parts = enc_key_str.split(".", 1)
        enc_type = int(parts[0])
        data = parts[1].split("|")

        if enc_type != 2:
            raise ValueError(f"Unsupported enc type for symmetric key: {enc_type}")

        iv = base64.b64decode(data[0])
        ct = base64.b64decode(data[1])
        mac = base64.b64decode(data[2]) if len(data) > 2 else None

        if mac is not None:
            mac_data = iv + ct
            expected = hmac.new(m_mac, mac_data, hashlib.sha256).digest()
            if not hmac.compare_digest(mac, expected):
                raise ValueError("MAC verification failed on symmetric key")

        cipher = Cipher(algorithms.AES(m_enc), modes.CBC(iv))
        decryptor = cipher.decryptor()
        padded = decryptor.update(ct) + decryptor.finalize()
        unpadder = padding.PKCS7(128).unpadder()
        dec_key = unpadder.update(padded) + unpadder.finalize()

        if len(dec_key) == 64:
            self._enc_key = dec_key[:32]
            self._mac_key = dec_key[32:]
        elif len(dec_key) == 32:
            self._enc_key, self._mac_key = self._stretch_key(dec_key)
        else:
            raise ValueError(f"Unexpected symmetric key length: {len(dec_key)}")

    # --- API calls ---

    def login(self, email: str, password: str) -> None:
        prelogin = self.session.post(
            self._api("api/accounts/prelogin"),
            json={"email": email},
        )
        prelogin.raise_for_status()
        kdf_iterations = prelogin.json().get("kdfIterations", 600000)

        master_key = self._make_master_key(password, email, kdf_iterations)
        master_hash = self._make_master_password_hash(master_key, password)

        resp = self.session.post(
            self._api("identity/connect/token"),
            data={
                "grant_type": "password",
                "username": email,
                "password": base64.b64encode(master_hash).decode(),
                "scope": "api offline_access",
                "client_id": "cli",
                "deviceType": "14",
                "deviceIdentifier": "python-client",
                "deviceName": "python",
            },
        )
        resp.raise_for_status()
        body = resp.json()
        self.access_token = body["access_token"]
        self.session.headers["Authorization"] = f"Bearer {self.access_token}"

        enc_key_str = body.get("Key") or body.get("key")
        if not enc_key_str:
            raise ValueError("No encrypted symmetric key in login response")
        self._decrypt_symmetric_key(enc_key_str, master_key)

        # Decrypt the user's RSA private key (needed for org key decryption)
        priv_key_str = body.get("PrivateKey") or body.get("privateKey")
        if priv_key_str:
            priv_key_der = self._decrypt_raw(priv_key_str)
            self._private_key = serialization.load_der_private_key(
                priv_key_der, password=None
            )

    def sync(self) -> dict:
        resp = self.session.get(self._api("api/sync"))
        resp.raise_for_status()
        return resp.json()

    def _load_org_keys(self, profile: dict) -> None:
        for org in profile.get("organizations", []):
            org_id = org.get("id")
            org_key_str = org.get("key")
            if not org_id or not org_key_str:
                continue
            # Org key may be RSA-encrypted (type 4) or AES-encrypted (type 2)
            dec = self._decrypt_raw(org_key_str)
            if len(dec) == 64:
                self._org_keys[org_id] = (dec[:32], dec[32:])
            elif len(dec) == 32:
                self._org_keys[org_id] = self._stretch_key(dec)

    def _decrypt_rsa(self, data: bytes) -> bytes:
        """Decrypt data using the user's RSA private key (OAEP with SHA-1)."""
        if self._private_key is None:
            raise ValueError("No RSA private key available for decryption")
        return self._private_key.decrypt(
            data,
            asym_padding.OAEP(
                mgf=asym_padding.MGF1(algorithm=hashes.SHA1()),
                algorithm=hashes.SHA1(),
                label=None,
            ),
        )

    def _decrypt_raw(self, cipher_string: str) -> bytes:
        """Decrypt a cipher string and return raw bytes (no UTF-8 decode)."""
        e_type, data = cipher_string.split(".", 1)
        e_type = int(e_type)

        if e_type == 4:
            # RSA-2048 OAEP SHA-1 — used for org keys
            return self._decrypt_rsa(base64.b64decode(data))

        if e_type != 2:
            raise ValueError(f"Unsupported encryption type: {e_type}")

        parts = data.split("|")
        iv = base64.b64decode(parts[0])
        ct = base64.b64decode(parts[1])
        mac = base64.b64decode(parts[2]) if len(parts) > 2 else None

        if mac is not None:
            mac_data = iv + ct
            expected = hmac.new(self._mac_key, mac_data, hashlib.sha256).digest()
            if not hmac.compare_digest(mac, expected):
                raise ValueError("MAC verification failed")

        cipher = Cipher(algorithms.AES(self._enc_key), modes.CBC(iv))
        decryptor = cipher.decryptor()
        padded = decryptor.update(ct) + decryptor.finalize()
        unpadder = padding.PKCS7(128).unpadder()
        return unpadder.update(padded) + unpadder.finalize()

    def _get_cipher_keys(self, cipher: dict) -> tuple[bytes, bytes]:
        """Return (enc_key, mac_key) for a cipher, using org key if applicable."""
        org_id = cipher.get("organizationId")
        if org_id and org_id in self._org_keys:
            return self._org_keys[org_id]
        return self._enc_key, self._mac_key

    def get_logins(self, search: str | None = None) -> list[dict]:
        data = self.sync()
        self._load_org_keys(data.get("profile", {}))
        results = []
        for cipher in data.get("ciphers", []):
            if cipher.get("type") != 1:  # 1 = Login
                continue
            enc_key, mac_key = self._get_cipher_keys(cipher)
            name = self._decrypt(cipher.get("name"), enc_key, mac_key)
            login = cipher.get("login", {})
            username = self._decrypt(login.get("username"), enc_key, mac_key)
            password = self._decrypt(login.get("password"), enc_key, mac_key)
            uri = ""
            uris = login.get("uris") or []
            if uris:
                uri = self._decrypt(uris[0].get("uri"), enc_key, mac_key)

            if (
                search
                and search.lower() not in name.lower()
                and search.lower() not in uri.lower()
            ):
                continue

            results.append(
                {
                    "name": name,
                    "username": username,
                    "password": password,
                    "uri": uri,
                }
            )
        return results


def main():
    parser = argparse.ArgumentParser(description="Query Vaultwarden vault items")
    parser.add_argument(
        "--url",
        required=True,
        help="Vaultwarden base URL (e.g. https://vault.example.com)",
    )
    parser.add_argument("--email", required=True, help="Account email")
    parser.add_argument("--search", "-s", default=None, help="Filter by name or URI")
    parser.add_argument("--json", action="store_true", help="Output as JSON")
    parser.add_argument(
        "--verify", action="store_true", help="Enable TLS certificate verification"
    )
    parser.add_argument(
        "--pass-file", default=None, help="Read master password from file"
    )
    args = parser.parse_args()

    if args.pass_file:
        with open(args.pass_file, "r") as f:
            password = f.read().strip()
    else:
        password = getpass("Master password: ")

    client = VaultwardenClient(args.url, verify_ssl=args.verify)
    try:
        client.login(args.email, password)
    except requests.HTTPError as e:
        print(f"Login failed: {e}", file=sys.stderr)
        sys.exit(1)

    logins = client.get_logins(search=args.search)

    if args.json:
        print(json.dumps(logins, indent=2))
    else:
        if not logins:
            print("No matching items found.")
            return
        for item in logins:
            print(f"Name:     {item['name']}")
            print(f"Username: {item['username']}")
            print(f"Password: {item['password']}")
            print(f"URI:      {item['uri']}")
            print("---")


if __name__ == "__main__":
    main()
