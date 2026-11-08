#!/usr/bin/env ruby
# Fastfile upload_verified 的回歸測試。
#
# 用途：上傳回報失敗時，upload_verified 應改查 ASC 上是否真的出現該 build 號，
# 而不是把 fastlane 的 altool 誤報當成失敗（成因見 Fastfile 內註解）。
#
# 執行（任何目錄皆可）：
#   ruby rust/fastlane/tests/upload_verified_test.rb
#
# 這個腳本完全不連線、不上傳：asc_build_present? 與 wait_for_asc_build 都被
# stub 掉，upload_to_app_store / upload_to_testflight 也被替換，所以不會誤觸
# 真實的 App Store 操作。唯一讀取的真實檔案是 macos/Info.plist（T7）。
require 'fileutils'

class UserError < StandardError; end

module UI
  def self.user_error!(msg) = raise(UserError, msg)
  def self.success(msg) = puts("[success] #{msg}")
  def self.important(msg) = puts("[important] #{msg}")
  def self.message(msg) = puts("[message] #{msg}")
end

# fastlane DSL stub：只提供 Fastfile 載入時會用到的部分
def default_platform(*); end
def app_store_connect_api_key(**); {}; end
def desc(*); end
def lane(*_a, &_b); end
def platform(*_a, &_b); end
def sh(*_a, **_k) = ''
def sleep(_n); end

FASTFILE = File.expand_path('../Fastfile', __dir__)
load FASTFILE

$checks = 0
$failures = 0

def pass(name)
  $checks += 1
  puts "PASS  #{name}"
end

def fail(name, why = nil)
  $checks += 1
  $failures += 1
  puts "FAIL  #{name}#{why ? " (#{why})" : ''}"
end

def expect_raise(name, fragment)
  yield
  fail(name, '沒有拋出錯誤')
rescue UserError => e
  e.message.include?(fragment) ? pass(name) : fail(name, "訊息不含 #{fragment.inspect}: #{e.message}")
end

# 測試替身
$calls = []
$present = false
$wait = false
$upload_error = nil

def asc_build_present?(_build_number)
  $calls << :present
  $present
end

def wait_for_asc_build(_build_number, **_kw)
  $calls << :wait
  $wait
end

def upload_to_testflight(**); end
def upload_to_app_store(**); end

def reset(present:, wait: false, error: 'Error uploading ipa/pkg file')
  $calls = []
  $present = present
  $wait = wait
  $upload_error = error
end

def do_upload
  upload_verified('7') do
    $calls << :upload
    raise $upload_error if $upload_error
  end
end

# T1：上傳回報失敗，但 ASC 有該 build -> 視為成功
reset(present: false, wait: true)
begin
  do_upload
  $calls == %i[present upload wait] ? pass('T1 ASC 有 build 時視為上傳成功') : fail('T1 呼叫順序異常', $calls.inspect)
rescue UserError => e
  fail('T1 ASC 有 build 時視為上傳成功', e.message)
end

# T2：上傳失敗且 ASC 一直沒有該 build -> 仍視為失敗
reset(present: false, wait: false)
expect_raise('T2 ASC 沒有 build 時仍視為失敗', '確實失敗') { do_upload }

# T3：該 build 號上傳前就存在 -> 不能宣稱本次成功
reset(present: true, wait: true)
expect_raise('T3 上傳前已存在的 build 不得誤判', '在上傳前就存在') { do_upload }

# T4：上傳成功 -> 不需要再查 ASC
reset(present: false, wait: true, error: nil)
begin
  do_upload
  $calls == %i[present upload] ? pass('T4 上傳成功時不多查 ASC') : fail('T4 呼叫順序異常', $calls.inspect)
rescue UserError => e
  fail('T4 上傳成功時不多查 ASC', e.message)
end

# T5：查不到 ASC（回 nil）-> 不猜測，要求人工核對
reset(present: nil, wait: nil)
expect_raise('T5 無法查詢 ASC 時要求人工核對', '無法向 ASC 確認') { do_upload }

# T6：讀不出 build 號 -> 直接中止
reset(present: false, wait: true)
expect_raise('T6 讀不出 build 號時中止', '讀不出本機 build 號') do
  upload_verified('') { $calls << :upload; raise 'boom' }
end

# T7：local_build_number 能讀到真實的 Info.plist
real = local_build_number
if real =~ /\A\d+\z/
  pass("T7 local_build_number 讀到真實值 #{real}")
else
  fail('T7 local_build_number 異常', real.inspect)
end

puts "\n#{$checks - $failures}/#{$checks} passed"
exit($failures.zero? ? 0 : 1)
