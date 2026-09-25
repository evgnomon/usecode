# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

def main() -> None:
    import uvicorn

    uvicorn.run("usecode_agent_api.app:app", host="0.0.0.0", port=8000, reload=True)
