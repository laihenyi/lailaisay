#!/usr/bin/env ruby
# 比對 lailaisay 與 NBLM2PPTX 兩份 Fastfile 的共用邏輯是否漂移。
#
# 背景：兩個 Mac app 的上傳驗證與 pkg 版本比對是同一套機制的兩份實作。
# NBLM2PPTX 是公開 repo，整個 macos-app/fastlane/ 在 .gitignore 內，沒有版控；
# 把邏輯抽成共用模組會讓它必須依賴 lailaisay 的目錄存在才能跑 fastlane，
# 反而更脆弱。因此保留兩份實作，改用這支測試確保它們不會走鐘。
#
# 執行（任何目錄皆可）：
#   ruby rust/fastlane/tests/fastfile_drift_test.rb
#
# 找不到對照檔時標示 SKIP 並以 0 結束（例如只 checkout 了單一 repo）。
# 可用環境變數覆寫位置：
#   DRIFT_BASELINE=/path/to/lailaisay/rust/fastlane/Fastfile
#   DRIFT_TARGET=/path/to/NBLM2PPTX/macos-app/fastlane/Fastfile
#
# 這個腳本只讀檔、不連線、不上傳。

BASELINE = ENV.fetch("DRIFT_BASELINE",
                     File.expand_path("../Fastfile", __dir__))
TARGET = ENV.fetch("DRIFT_TARGET",
                   File.expand_path("../../../../NBLM2PPTX/macos-app/fastlane/Fastfile", __dir__))

# 兩份 Fastfile 應該一致的函式
SHARED = %w[asc_build_present? wait_for_asc_build upload_verified pkg_versions verify_pkg_version!].freeze

$checks = 0
$failures = 0

def pass(name)
  $checks += 1
  puts "PASS  #{name}"
end

def fail(name, detail = nil)
  $checks += 1
  $failures += 1
  puts "FAIL  #{name}"
  Array(detail).each { |l| puts "      #{l}" }
end

# 取一個 def ... end 的完整內容。
# 只把「行首」的 if/unless/case/def 當成區塊開頭——行尾的修飾用法
# （`return nil unless app`）不是區塊，算進去會讓範圍一路吃過函式結尾。
def extract_def(src, name)
  lines = src.lines
  start = lines.index { |l| l.start_with?("def #{name}(") || l.start_with?("def #{name} ") }
  return nil unless start

  depth = 0
  out = []
  lines[start..].each do |l|
    out << l
    depth += 1 if l =~ /^\s*(def|class|module|if|unless|case|begin|while|until|for)\b/
    depth += 1 if l =~ /\bdo\s*(\|[^|]*\|)?\s*$/ && l !~ /^\s*(while|until|for)\b/
    depth -= 1 if l =~ /^\s*end\b/
    break if depth <= 0
  end
  out.join
end

# 抹掉「產品專屬、本來就應該不同」的部分，只留邏輯骨架：
#   - 註解：兩邊說明各自產品的背景，不是邏輯
#   - 憑證來源：lailaisay 用常數，NBLM2PPTX 用 env!()（公開 repo 不存憑證）
#   - 本機版本讀取函式命名：local_build_number / local_build
#   - 字串內容：錯誤訊息描述各自產品，只保留「這裡有字串」
def normalize(body)
  b = body.dup
  b = b.lines.reject { |l| l =~ /^\s*#/ }.join # 整行註解
  b.gsub!(/\s#\s.*$/, "")                        # 行尾註解
  b.gsub!(/key_id:.*?filepath:\s*\S+\),?/m, "key_id: CRED)")
  b.gsub!(/^\s*(key_id|issuer_id|filepath):.*$/, '\1: CRED')
  b.gsub!(/\blocal_build_number\b/, "LOCAL_BUILD")
  b.gsub!(/\blocal_build\b(?!_)/, "LOCAL_BUILD")
  b.gsub!(/LOCAL_BUILD\.to_s/, "LOCAL_BUILD")
  b.gsub!(/marketing_version\.to_s/, "marketing_version")
  b.gsub!(/"(?:[^"\\]|\\.)*"/, "STR")
  b.gsub!(/\s+/, " ").strip
end

# 指出兩段文字從哪裡開始不同，避免只說「不一樣」還得自己去比。
def first_divergence(a, b)
  aw = a.split(" ")
  bw = b.split(" ")
  i = 0
  i += 1 while i < aw.size && i < bw.size && aw[i] == bw[i]
  [
    "第 #{i + 1} 個 token 起不同：",
    "  lailaisay : ...#{aw[[i - 3, 0].max, 8].join(' ')}",
    "  NBLM2PPTX: ...#{bw[[i - 3, 0].max, 8].join(' ')}"
  ]
end

unless File.exist?(BASELINE)
  abort "找不到 baseline Fastfile：#{BASELINE}"
end
unless File.exist?(TARGET)
  puts "SKIP  找不到對照 Fastfile：#{TARGET}"
  puts "      （只 checkout 單一 repo 時屬正常；可用 DRIFT_TARGET 指定路徑）"
  puts "\n0/0 passed (1 skipped)"
  exit 0
end

baseline_src = File.read(BASELINE)
target_src = File.read(TARGET)

SHARED.each do |fn|
  a = extract_def(baseline_src, fn)
  b = extract_def(target_src, fn)

  if a.nil? || b.nil?
    fail("#{fn} 兩邊都存在", [
           "#{a.nil? ? 'lailaisay 缺少' : ''}#{b.nil? ? ' NBLM2PPTX 缺少' : ''} 這個函式" \
           "（新增或刪除共用函式時，兩邊要一起改）"
         ])
    next
  end

  na = normalize(a)
  nb = normalize(b)
  if na == nb
    pass("#{fn} 邏輯一致")
  else
    fail("#{fn} 邏輯已漂移", first_divergence(na, nb) + [
      "改了其中一份就要同步另一份；若差異是刻意的，更新本測試的 normalize 規則。"
    ])
  end
end

# 取出單一 lane 的內容（從 `lane :name do` 到下一個同層 `  end`）。
# 用整個檔案做 include? 會漏掉「只從其中一條 lane 拿掉」的情況。
def extract_lane(src, name)
  lines = src.lines
  start = lines.index { |l| l.strip.start_with?("lane :#{name} do") }
  return nil unless start

  out = []
  lines[start..].each do |l|
    break if out.any? && l == "  end\n"
    out << l
  end
  out << "  end\n" if out.any? && out.last != "  end\n"
  out.join
end

# 兩邊的 beta / release 都應該呼叫 pkg 版本比對，避免只加在其中一條 lane。
%w[beta release].each do |lane|
  a = extract_lane(baseline_src, lane)
  b = extract_lane(target_src, lane)

  if a.nil? || b.nil?
    fail("#{lane} lane 兩邊都存在", ["找不到 lane :#{lane}"])
    next
  end

  ha = a.include?("verify_pkg_version!(PKG_PATH)")
  hb = b.include?("verify_pkg_version!(PKG_PATH)")
  if ha && hb
    pass("#{lane} lane 都有 pkg 版本比對")
  else
    fail("#{lane} lane 兩邊都要有 pkg 版本比對",
         ["#{ha ? '' : 'lailaisay '}#{hb ? '' : 'NBLM2PPTX '}這條 lane 沒有呼叫 verify_pkg_version!"])
  end
end

puts "\n#{$checks - $failures}/#{$checks} passed"
exit($failures.zero? ? 0 : 1)
