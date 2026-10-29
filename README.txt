License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)

usecode - Container for agents
==============================

Use code! When your coding agent can run code, it solves problems better and
burns fewer tokens doing it, so it has more time left for the interesting
stuff.

To run code, an agent needs a place to run it; it can't work in a void.
usecode is that container: machines your agent can spin up, tools it can
run, and an API it can call.

usecode is open source and freely distributed. The code is right here.

The easiest way to use it is usecode.dev. It's already set up and kept
running, so your agents can start working today:

  1. Sign in at https://usecode.dev (free for individuals) and get an API key.
  2. Hand it to your agent. For Claude Code that's one line:

       claude mcp add usecode -e USECODE_MCP_API_KEY=<your key> -- uc-agent-mcp

Cursor, Codex, Gemini CLI, opencode and friends, see:
https://github.com/evgnomon/usecode/blob/master/docs/coding-agents.md

Upgrades, backups and uptime are on us, not your weekend.


Running it yourself
-------------------

The code is here under the HGL General License (COPYING). On a Debian or
Ubuntu box, put the `uc` and `uc-sync` binaries on your PATH, then:

  uc sync -t usecode -d

It clones this repository, installs what the build needs, then builds and
installs the rest of the tools. Then run `uc help`.

Everything usecode.dev hosts, like monitoring, the audit log and backups,
you can run yourself too; it just takes machines and someone watching them.
If you'd rather we make sure it doesn't break, we're happy to talk:
https://usecode.dev


Contributing
------------

Patches are welcome. See CONTRIBUTING.md (commits need a Signed-off-by line)
and CODE_OF_CONDUCT.md.


No Warranty
-----------

The following disclaimers must keep being prominently displayed in the
documentation and any other materials for the Covered Work or a Derivative
Work:

EXCEPT WHEN OTHERWISE STATED IN WRITING, THE COPYRIGHT HOLDERS AND/OR OTHER
PARTIES PROVIDE THE COVERED WORK "AS IS" WITHOUT IMPLIED WARRANTIES OF FITNESS
FOR A PARTICULAR PURPOSE, NON-INFRINGEMENT, MERCHANTABILITY AND TITLE, AND ANY
OTHER KIND OF EXPRESSED OR IMPLIED WARRANTIES.

IN NO EVENT SHALL ANY COPYRIGHT HOLDER, OR ANY OTHER PARTY WHO MAY MODIFY
AND/OR REDISTRIBUTE THE PROGRAM AS PERMITTED ABOVE BE LIABLE FOR ANY DIRECT,
INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
LOSS OF GOODWILL, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THE
COVERED WORK OR AS A RESULT OF OUR LICENSE, EVEN IF ADVISED OF THE POSSIBILITY
OF SUCH DAMAGE.

Nevertheless, you retain the option to extend offers of support, warranties,
indemnities, or other liability obligations and/or rights in alignment with
this License. Such offers may be provided in exchange for a fee, at your
discretion. This provision allows you to engage in commercial transactions by
offering additional services or assurances while remaining compliant with the
terms of this License.
