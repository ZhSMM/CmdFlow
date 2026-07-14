-- 0003: executions 增加 rendered_template 列
--
-- 背景：历史详情之前只展示 input_params + node_runs，看不到执行时
-- 真正传给 shell 的命令。input_params 是用户填的值，模板可能做了
-- 字符串拼接、shell 转义、跨字段引用，调试时不看到最终拼好的命令
-- 很难定位为什么某个参数没生效。
--
-- 解决：执行时把渲染好的最终命令存下来，详情直接展示。
-- workflow 类型的执行不是单条命令，存「工作流名 + 节点数」摘要也够用。

ALTER TABLE executions ADD COLUMN rendered_template TEXT;
