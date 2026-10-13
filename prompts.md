 every features should exactly be like Notion but keep the project name as something else. Tech stack - for frontend I think they use Next js and figure out other stacks (come up optimal one)

optimize the guardrails for AI Agent. Do not elaborate it or exaggerate it. keep it short and simple


keep in mind that Orchestrator should not just give the other agents a big chunk of work [because the model has only 300k context window], instead it should iteratively build the application using these agents

Github Actions CI is the only compile and Build authority, there is no local runtime. By default use Blueprints skill for  managing context. first come up with a plan and write Agent file for cursor. Build our application with aesthetics and intuitive UI. 


architect doesn't going to run build commands, it's job is to calculate the feasibility, trade-offs and future scope for better scalability and maintainbility
I have described the Agent requirements vaguely, coming up with best config is your job

considering the following, whil agent file, 







```
cd C:/nike-tracker; git tag -a v0.1.1 -m "v0.1.1 - debug-signed, sideloadable release APK"; git push origin v0.1.1
```



If there is any architectural flaws/quirks - come up with a plan. Only proceeed further if it is approved. 
you can always propose optimal implementation than in the blueprint , only proceed further if user say approved it

This project right now has many flaws, a lot features / functionalities isn't working as expected. instead of directly editing it, ask questions of "how it could have been implemented better?" or "Stick with the existing system."


Stick with the existing Privileged broker and cleanup [as described in blueprints].
For [clean up for browser, system, temp files and so on] clearing system-level files - on a scheduled date and time. I think there is no UI for user to set these things up. so Create one




--------------


# Guardrails


- Never use the word (`N-I-K-E`, any case) in codebase, file creation, names, or commits.
- GitHub Actions CI is the only build authority. Nothing runs locally.
- Read CI logs with `gh-cli`. If `gh-cli` is unavailable, stop and ask me for the logs.
- Never read `prompts.md`.


- engineering practices (top level comment, func comments)
- gh-cli
- Build production-grade code only. No mocked UI or placeholder data.
- Use the Blueprints skill to manage context.

- Ignore the root folder and the git folder name; just get the context from README.md and archictecture.md
- Never include Actions Run ID as verification status in BLUEPRINTs
- assets /.diagrams /wireframes 



## Git commands
```
Remove-Item -Recurse -Force .git
rmdir /s /q .git

git init
git log --oneline

echo "# Nike-op" >> README.md
git init
git add README.md
git commit -m "first commit"
git branch -M main
git remote add origin https://github.com/<>.git
git push -u origin main

```



## Migration Checklist

- [ ] Blueprints Skill
- [ ] AGENTS.md, AGENTs folder
- [ ] prompts.md
- [ ] Design files / Wireframes (Excalidraw)



## PROMPTS

### AGENT Instruction definition guidelines

- AGENT instruction should not be - too descriptive/ specific or too vague, It should be inbetween somewhere.
- Do not elaborate or exaggerate instruction for the AGENT; keep it clean and concise



## CURSOR

- /summarize, /visualize

- Execution and Approvals > Auto-Review

- `is_background: true` — the main agent starts the subagent and keeps working without waiting for it to finish.

- Fork - Agents Window > Right Click

- Custom Commands `.cursor/commands/command.md`

- Keyboard Shortcuts —— `Cmd+I` (open agent), `Cmd+/` (change models)



**Plan Mode** Use this mode to have the AI research your codebase and generate a structured implementation plan before writing code.




--------------


## Agents 


## AGENT HQ

we already have agents [Research, Designer, Architect, Coder, Planner, Reviewer]. find the opitimal model for these from the available list.
Design specialized subagent for Cursor [ Reviewer]. Describe its role and how it should respond. Give description for each agent (cursor) and if additional property required - add them as well.



Research Agent - Research for the existing product's system, features.

Designer - responsible for designing Aesthetics and intuitive UI,  UI workflows. Returns Design plan with granular details. currently it writes code

Coder - writes code consider every possible edge cases

Productmind - Researches [external media like relevant subreddit page, ] what existing product failed address or could have done better that speaks for the product itself. can only be called by {Designer, Research Agent}

Reviewer - reviews code by build error check via gh-cli CI logs, polls and returns what actually causing build fails/ errors

Planner - comes up with a plan, proceeds further if it is approved. writes plan in md file and wait for approval. Planner can invoke these Agents {Architect, Researcher, productmind}


---

write all the Agent config in an .md file


subAgent - Research, review through CI logs


- Also come up with the list of best model that are best fit that specific role. [Do web research, get data from only legit sites. Do not spawn subAgents for this task]


### SubAgent models:

- **Claude:** Opus 4.5 (high thinking), Sonnet 4.5 (thinking), Sonnet 4, Haiku 4.5 (thinking)
- **GPT:** 5.1, 5.2, 5.3 Codex, 5.4 Mini, 5.4 Nano, 5 Mini
- **Gemini:** 3.1 Pro, 3.5 Flash, 3 Flash, 2.5 Flash
- **Others:** Kimi K3, Kimi K2.7 Code, GLM 5.2, Composer 2.5

ROADMAP.md

-Phases of implementation plan. It should have iterative improvement and refinement of features and workflows... with granular details 


In the Agent Orchestration workflow, the main agent has to write code here after (No coder Agent). Do not write instructions like you will have to write code, blah blah blah. keep it short and simple and remove Coder Agent from AGENTS.md. 



