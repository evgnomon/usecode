# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

def main() -> None:
    import uvicorn

    uvicorn.run("usecode_dev_site.app:app", host="0.0.0.0", port=8080, reload=True)
