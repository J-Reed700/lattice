# CLAUDE.md - AI Assistant Guide for Recall

## AI Assistant Guidelines

### **🔴 THE SACRED RULES - MUST FOLLOW ALWAYS 🔴**

**THESE RULES ARE MANDATORY FOR EVERY TASK - NO EXCEPTIONS**

When implementing ANY feature or change, you MUST follow this workflow:

#### **1. ANALYZE** 🔍
- **Use `zen-architect` agent in ANALYZE mode** to break down the problem
- Understand requirements, constraints, and architecture
- Create a detailed implementation plan
- Identify all affected components

#### **2. ARCHITECT** 🏗️
- **Use `zen-architect` agent in ARCHITECT mode** for system design
- Design the solution following DDD, SOLID, and project patterns
- Specify interfaces, data models, and interactions
- Create specifications for modular-builder

#### **3. IMPLEMENT** ⚙️
- **Use `modular-builder` agent** to implement the specifications
- Follow the architecture and specifications exactly
- Never skip steps or deviate from the plan
- Complete ALL tasks identified in ANALYZE phase

#### **4. VERIFY** ✅
- **Use `zen-architect` agent in REVIEW mode** to verify the implementation
- Ensure code quality, security, and compliance
- Check all requirements are met
- Verify tests pass and coverage is adequate
- **VERIFY BUILD SUCCEEDS** - Run `cargo build` (Rust) or `npm run build` (TypeScript) or `poetry run pytest` (Python)
- Check for compilation errors, type errors, and test failures

#### **5. FIX (if needed)** 🔧
- If verification fails, use appropriate agents to fix issues
- Return to step 4 (VERIFY) after fixes
- Repeat until verification passes

#### **6. NEVER COMMIT** 📝
- Only commit once given express permission to
- Use conventional commit messages
- Include all related changes

**WARNING**:
- ❌ NEVER skip verification - it catches critical issues
- ❌ NEVER implement without analysis and architecture first
- ❌ NEVER commit without verification passing
- ❌ Always use the specified agents - don't try to do it yourself

**Example Workflow**:
```
User: "Add download tracking for models"

1. ANALYZE: zen-architect analyzes requirements
2. ARCHITECT: zen-architect designs the solution
3. IMPLEMENT: modular-builder builds it
4. VERIFY: zen-architect reviews implementation
   - If issues found: fix with bug-hunter/modular-builder, then re-verify
   - If verified: proceed to commit
5. COMMIT: Create commit with changes
```

**Remember**: These rules protect code quality, security, and maintainability. Following them saves time and prevents bugs.

---

<!-- ORACLE-RECOVERY -->
## Oracle Recovery

If context was compacted mid-task, check `FULLAUTO_CONTEXT.md` for task state and run `/fullauto` to resume.
