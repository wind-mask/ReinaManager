#!/usr/bin/env -S nu --stdin

# 用于 git rebase/bisect 的代码检查脚本
# 使用方式:
#   ./scripts/check.nu          # 完整检查 (前端 + 后端)
#   ./scripts/check.nu quick    # 快速检查 (仅 cargo check，跳过 clippy)
#   ./scripts/check.nu fe       # 仅检查前端
#   ./scripts/check.nu be       # 仅检查后端
#   ./scripts/check.nu lint     # 仅运行 lint (不修改文件)
#
# git bisect 示例:
#   git bisect start
#   git bisect bad HEAD
#   git bisect good v0.10.0
#   git bisect run ./scripts/check.nu quick

# 前端检查：lint + 类型检查
def check_frontend [] {
    print $"(ansi cyan)>>> 前端检查(ansi reset)"
    
    # biome lint (不自动修复，仅检查)
    print $"(ansi yellow)  -> biome check(ansi reset)"
    let lint_result = (do { bun biome check . } | complete)
    if $lint_result.exit_code != 0 {
        print $"(ansi red)biome check 失败(ansi reset)"
        print $lint_result.stderr
        return 1
    }
    
    # TypeScript 类型检查
    print $"(ansi yellow)  -> tsgo 类型检查(ansi reset)"
    let type_result = (do { tsgo -b --noEmit } | complete)
    if $type_result.exit_code != 0 {
        print $"(ansi red)类型检查失败(ansi reset)"
        print $type_result.stderr
        return 1
    }
    
    print $"(ansi green)前端检查通过(ansi reset)"
    return 0
}

# 后端检查：cargo check + clippy
def check_backend [quick: bool = false,target?:string] {
    print $"(ansi cyan)>>> 后端检查(ansi reset)"
    print target=($target) 
    cd src-tauri
    
    # cargo check
    print $"(ansi yellow)  -> cargo check(ansi reset)"
    mut check_result = {}; 
    if ($target | is-not-empty) {
        print 检查命令为"cargo check --target ($target)"
                $check_result = (do { cargo check --target $target } | complete)
    } else {
     $check_result = (do { cargo check --all-targets } | complete)
    }
    if $check_result.exit_code != 0 {
        print $"(ansi red)cargo check 失败(ansi reset)"
        print $check_result.stderr
        return 1
    }
    
    # clippy (快速模式跳过)
    if not $quick {
        print $"(ansi yellow)  -> cargo clippy(ansi reset)"
        mut clippy_result = {}
        if ($target | is-not-empty) {
            print clippy命令为"cargo clippy --target ($target) -- -D warnings"
$clippy_result = (do { cargo clippy --target $target -- -D warnings } | complete)
        } else {
         $clippy_result = (do { cargo clippy --all-targets -- -D warnings } | complete)
        }
        if $clippy_result.exit_code != 0 {
            print $"(ansi red)clippy 检查失败(ansi reset)"
            print $clippy_result.stderr
            return 1
        }
    } else {
        print $"(ansi dim)  -> 跳过 clippy (快速模式)(ansi reset)"
    }
    
    print $"(ansi green)后端检查通过(ansi reset)"
    return 0
}

# 仅 lint 检查
def check_lint [] {
    print $"(ansi cyan)>>> Lint 检查(ansi reset)"
    
    print $"(ansi yellow)  -> biome check(ansi reset)"
    let lint_result = (do { biome check . } | complete)
    if $lint_result.exit_code != 0 {
        print $"(ansi red)biome check 失败(ansi reset)"
        print $lint_result.stderr
        return 1
    }
    
    print $"(ansi green)Lint 检查通过(ansi reset)"
    return 0
}

def main [
    mode?: string  # 检查模式: quick, fe, be, lint (默认完整检查),
    --target: string # rust target,默认当前，可指定
] {
    let start_time = (date now)
    print $"(ansi magenta_bold)====== 开始检查 ======(ansi reset)"
    
    let result = match $mode {
        "quick" => {
            # 快速模式：前端 + 后端 (跳过 clippy)
            let fe = (check_frontend)
            if $fe != 0 { $fe } else { check_backend true $target }
        }
        "fe" => {
            check_frontend
        }
        "be" => {
            check_backend false
        }
        "lint" => {
            check_lint
        }
        null | "" => {
            # 完整检查
            let fe = (check_frontend)
            if $fe != 0 { $fe } else { check_backend false $target}
        }
        _ => {
            print $"(ansi red)未知模式: ($mode)(ansi reset)"
            print "可用模式: quick, fe, be, lint"
            1
        }
    }
    
    let elapsed = ((date now) - $start_time)
    
    if $result == 0 {
        print $"(ansi green_bold)====== 检查通过 ✓ 耗时: ($elapsed) ======(ansi reset)"
    } else {
        print $"(ansi red_bold)====== 检查失败 ✗ 耗时: ($elapsed) ======(ansi reset)"
    }
    
    exit $result
}
